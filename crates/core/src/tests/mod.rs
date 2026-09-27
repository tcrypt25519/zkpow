use alloc::vec::Vec;

#[cfg(not(target_os = "zkvm"))]
extern crate std;

mod halt;
mod input;
mod mtp;
mod pow;
mod public_values;
mod retarget;
mod state;

/// The transition is generic over the network, so its tests drive it with the real ones.
/// `ShortEpoch` differs from `Mainnet` only in `epoch_length`, which is what makes it useful here.
use zkpow_params::Mainnet as TestConsensus;

use super::*;

pub(crate) const GENESIS_NBITS: u32 = *TestConsensus::PARAMS.genesis.compact_target.as_inner();
pub(crate) const GENESIS_TARGET: Target = TestConsensus::PARAMS.genesis_target;

pub(crate) const EXPECTED_EPOCH_TIMESPAN: u32 = TestConsensus::PARAMS.expected_epoch_time();
pub(crate) const MIN_EPOCH_TIMESPAN: i64 = TestConsensus::PARAMS.min_epoch_timespan();
pub(crate) const MAX_EPOCH_TIMESPAN: i64 = TestConsensus::PARAMS.max_epoch_timespan();

pub(crate) fn expect_error_code(e: ApplyError) -> ErrorCode {
    e.error_code().expect("All ApplyErrors return an ErrorCode")
}

#[must_use]
pub(crate) fn set_hash_success() -> crate::test_hash_mock::MockHashGuard {
    crate::test_hash_mock::set_success()
}

#[must_use]
pub(crate) fn set_hash_fail() -> crate::test_hash_mock::MockHashGuard {
    crate::test_hash_mock::set_fail()
}

pub(crate) fn ts(seconds: u32) -> BlockTimestamp {
    BlockTimestamp::new(seconds)
}

pub(crate) fn zero_hash() -> BlockHash {
    BlockHash::new([0; 32])
}

pub(crate) fn sample_header() -> Header {
    Header {
        version:        1,
        prev_blockhash: BlockHash::new([0x11; 32]),
        merkle_root:    [0x22; 32],
        timestamp:      ts(10),
        compact_target: CompactTarget::new(GENESIS_NBITS),
        nonce:          7,
    }
}

pub(crate) fn hash_sample_header() -> BlockHash {
    hash_header(&sample_header())
}

pub(crate) fn test_state() -> State {
    State {
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        ..State::default()
    }
}

pub(crate) fn median_time_past(state: &State) -> BlockTimestamp {
    let count = timestamp_count(state.height);
    let mut sorted = state.timestamps;
    if count >= WINDOW_SIZE {
        sorted.sort_unstable();
        sorted[WINDOW_SIZE / 2]
    } else {
        sorted[..count].sort_unstable();
        sorted[count / 2]
    }
}

pub(crate) fn dense_median_hints_for_headers(initial_state: &State, headers: &[HeaderInput]) -> Vec<BlockTimestamp> {
    let mut state = *initial_state;
    let mut medians = Vec::with_capacity(headers.len());
    for header in headers {
        medians.push(median_time_past(&state));
        let timestamp_slot = (state.height as usize + 1) % WINDOW_SIZE;
        state.timestamps[timestamp_slot] = header.timestamp;
        state.height += 1;
    }
    medians
}

pub(crate) fn median_hints_for_headers(initial_state: &State, headers: &[HeaderInput]) -> Vec<BlockTimestamp> {
    let dense_hints = dense_median_hints_for_headers(initial_state, headers);
    let state = *initial_state;
    let warm_start = if state.height == 0 {
        (WINDOW_SIZE - 1).min(headers.len())
    } else {
        0
    };
    let mut needed = Vec::with_capacity(headers.len());
    let mut ordered_run = ordered_timestamp_run(&state.timestamps, state.height);
    let mut previous_timestamp = if state.height == 0 {
        None
    } else {
        Some(state.timestamps[timestamp_slot(state.height)].into_inner())
    };

    for (index, header) in headers.iter().enumerate() {
        if index >= warm_start && !has_ordered_timestamp_window(ordered_run) {
            needed.push(dense_hints[index]);
        }

        ordered_run = advance_ordered_timestamp_run(
            ordered_run,
            previous_timestamp.unwrap_or(header.timestamp.into_inner()),
            header.timestamp.into_inner(),
        );
        previous_timestamp = Some(header.timestamp.into_inner());
    }

    needed
}

pub(crate) fn failure_public_value_bytes(state: &State, error_code: ErrorCode) -> [u8; PublicValues::SIZE] {
    let mut bytes = [0u8; PublicValues::SIZE];
    bytes[0..State::SIZE].copy_from_slice(bytemuck::bytes_of(state));
    bytes[State::SIZE..State::SIZE + 32].copy_from_slice(bytemuck::bytes_of(&VerifierKeyDigest::default()));
    bytes[PublicValues::SIZE - 1] = ReturnCode::from(error_code).into_u8();
    bytes
}

pub(crate) fn expected_upper_median(height: u32) -> BlockTimestamp {
    if height < WINDOW_SIZE as u32 {
        ts(height.div_ceil(2))
    } else {
        ts(height - (WINDOW_SIZE as u32 / 2))
    }
}

pub(crate) fn ordered_warm_state(height: u32) -> State {
    let _hash_guard = set_hash_success();
    let mut state = test_state();
    let timestamps: Vec<u32> = (1..=height).map(|step| 1_000 + step * 600).collect();
    let headers = headers_from_timestamps(&timestamps);
    let hints = median_hints_for_headers(&state, &headers);
    state.apply_headers_from_genesis::<TestConsensus>(&headers, &hints).unwrap();
    state
}

pub(crate) fn headers_from_timestamps(timestamps: &[u32]) -> Vec<HeaderInput> {
    timestamps
        .iter()
        .map(|timestamp| HeaderInput {
            version:     1,
            merkle_root: [0x22; 32],
            timestamp:   ts(*timestamp),
            nonce:       7,
        })
        .collect()
}

pub(crate) fn apply_sequentially(state: &mut State, headers: &[HeaderInput]) -> Result<(), ApplyError> {
    for header in headers {
        let hints = median_hints_for_headers(state, core::slice::from_ref(header));
        state.apply_headers::<TestConsensus>(core::slice::from_ref(header), &hints)?;
    }
    Ok(())
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ParsedBlock {
    pub(crate) expected_hash:  BlockHash,
    pub(crate) version:        u32,
    pub(crate) prev_blockhash: BlockHash,
    pub(crate) merkle_root:    [u8; 32],
    pub(crate) timestamp:      u32,
    pub(crate) compact_target: u32,
    pub(crate) nonce:          u32,
}

pub(crate) fn parse_fixture_blocks(fixture_bytes: &[u8]) -> &[ParsedBlock] {
    bytemuck::cast_slice(fixture_bytes)
}
