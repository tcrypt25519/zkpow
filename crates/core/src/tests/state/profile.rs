//! Opt-in mainnet epoch profile. Compare the total VM cycles of these three
//! tests to isolate the final boundary header and the ordinary header after it.
//! The differences also include any length-dependent setup and finalization.
//! Run `cargo zkvm test -p zkpow-core --features epoch-profile --release profile_ --profile`.
//! Regenerate the fixture with `python3 scripts/generate-epoch-profile-fixture.py --epochs 3`.

use core::mem::size_of;

use zkvm_test::zkvm_test;

use crate::{tests::*, BlockTimestamp, ChainWork, HeaderInput, State};

/// Change this to profile more complete epochs (up to the fixture's capacity).
const PROFILE_EPOCHS: usize = 1;
const EPOCH_LENGTH: usize = 2016;

fn run_mainnet_prefix(header_count: usize) {
    assert_eq!(TestConsensus::PARAMS.epoch_length as usize, EPOCH_LENGTH);
    assert!(PROFILE_EPOCHS > 0);

    // The fixture stores a count, real HeaderInputs, prefix counts for sparse
    // median hints, then the hints. Parsing all of it costs the same in each run.
    let fixture = zkvm_test::fixture();
    let fixture_count = u32::from_le_bytes(fixture[..4].try_into().unwrap()) as usize;
    assert!(
        fixture_count >= PROFILE_EPOCHS * EPOCH_LENGTH + 1,
        "regenerate the fixture with more epochs"
    );
    let headers_end = 4 + fixture_count * HeaderInput::SIZE;
    let offsets_end = headers_end + (fixture_count + 1) * size_of::<u32>();
    let all_headers: &[HeaderInput] = bytemuck::cast_slice(&fixture[4..headers_end]);
    let hint_offsets: &[u32] = bytemuck::cast_slice(&fixture[headers_end..offsets_end]);
    let all_hints: &[BlockTimestamp] = bytemuck::cast_slice(&fixture[offsets_end..]);
    assert_eq!(all_hints.len(), hint_offsets[fixture_count] as usize);
    let headers = &all_headers[..header_count];
    let hints = &all_hints[..hint_offsets[header_count] as usize];

    let mut state = State {
        chain_work:      ChainWork::from_limbs(*TestConsensus::PARAMS.genesis_work.as_limbs()),
        genesis_hash:    TestConsensus::GENESIS_HASH,
        block_hash:      TestConsensus::GENESIS_HASH,
        height:          0,
        current_nbits:   TestConsensus::PARAMS.genesis.compact_target,
        epoch_timestamp: TestConsensus::PARAMS.genesis.timestamp,
        timestamps:      {
            let mut values = [BlockTimestamp::new(0); WINDOW_SIZE];
            values[0] = TestConsensus::PARAMS.genesis.timestamp;
            values
        },
    };

    state.apply_headers_from_genesis::<TestConsensus>(headers, hints).unwrap();
    assert_eq!(state.height as usize, header_count);
}

#[zkvm_test(fixture = "epoch_profile_headers.bin")]
fn profile_before_epoch_boundary() {
    run_mainnet_prefix(PROFILE_EPOCHS * EPOCH_LENGTH - 1);
}

#[zkvm_test(fixture = "epoch_profile_headers.bin")]
fn profile_at_epoch_boundary() {
    // At state height N*2016-1, this header prepares the next target.
    run_mainnet_prefix(PROFILE_EPOCHS * EPOCH_LENGTH);
}

#[zkvm_test(fixture = "epoch_profile_headers.bin")]
fn profile_mainnet_epochs_and_next_header() {
    // This header applies normally under the target prepared at the boundary.
    run_mainnet_prefix(PROFILE_EPOCHS * EPOCH_LENGTH + 1);
}
