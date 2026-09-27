use zkpow_params::ShortEpoch;
use zkvm_test::zkvm_test;

use crate::{
    calculate_next_target_required, target_from_bits, target_to_bits, tests::*, CompactTarget, Consensus, HeaderInput,
    Target,
};

#[zkvm_test(max_cycles = 150_000)]
fn target_to_bits_round_trips_genesis_target() {
    let round_trip = target_to_bits(GENESIS_TARGET);
    assert_eq!(round_trip, CompactTarget::new(GENESIS_NBITS));
}

#[zkvm_test(max_cycles = 150_000)]
fn target_from_bits_expands_genesis_bits() {
    let expanded = target_from_bits(CompactTarget::new(GENESIS_NBITS));
    assert_eq!(expanded, GENESIS_TARGET);
}

#[zkvm_test]
fn retarget_timespan_cases_adjust_target_direction() {
    enum ExpectedDirection {
        Equal,
        NotGreater,
        NotLess,
    }

    let cases = [
        ("expected timespan", EXPECTED_EPOCH_TIMESPAN, ExpectedDirection::Equal),
        ("shorter timespan", EXPECTED_EPOCH_TIMESPAN / 4, ExpectedDirection::NotGreater),
        ("longer timespan", EXPECTED_EPOCH_TIMESPAN * 4, ExpectedDirection::NotLess),
        (
            "minimum clamped timespan",
            MIN_EPOCH_TIMESPAN as u32,
            ExpectedDirection::NotGreater,
        ),
        (
            "maximum clamped timespan",
            MAX_EPOCH_TIMESPAN as u32,
            ExpectedDirection::NotLess,
        ),
    ];

    for (name, timespan, expected) in cases {
        let old = GENESIS_TARGET;
        let (bits, new, _) = calculate_next_target_required::<TestConsensus>(old, ts(0), ts(timespan));

        match expected {
            ExpectedDirection::Equal => {
                assert_eq!(bits, CompactTarget::new(GENESIS_NBITS), "{name}: bits");
                assert_eq!(new, old, "{name}: target");
            },
            ExpectedDirection::NotGreater => {
                assert!(new <= old, "{name}: target should be <= old");
            },
            ExpectedDirection::NotLess => {
                assert!(new >= old, "{name}: target should be >= old");
            },
        }
    }
}

#[zkvm_test]
fn retargeting_clamping_bounds_match_fourfold_limits() {
    let old_target = Target::from_limbs([0, 0, 0, 0x0000_0000_7FFF_0000]);

    // 100 days vs 56 days (clamped upper bound: 4x expected timespan)
    let (_, target_100_days, work_100_days) =
        calculate_next_target_required::<TestConsensus>(old_target, ts(0), ts(100 * 86400));
    let (_, target_56_days, work_56_days) =
        calculate_next_target_required::<TestConsensus>(old_target, ts(0), ts(56 * 86400));
    assert_eq!(target_100_days, target_56_days, "timespan > 4x must clamp to 4x");
    assert_eq!(work_100_days, work_56_days);

    // 1 day vs 3.5 days (clamped lower bound: 0.25x expected timespan)
    let (_, target_1_day, work_1_day) = calculate_next_target_required::<TestConsensus>(old_target, ts(0), ts(86400));
    let (_, target_quarter, work_quarter) =
        calculate_next_target_required::<TestConsensus>(old_target, ts(0), ts(302400));
    assert_eq!(target_1_day, target_quarter, "timespan < 0.25x must clamp to 0.25x");
    assert_eq!(work_1_day, work_quarter);

    // Excessively slow retarget clamped to genesis pow_limit
    let slow_target = Target::from_limbs([0, 0, 0, 0x0000_0000_FFFF_0000]);
    let (nbits_clamped, target_clamped, _) =
        calculate_next_target_required::<TestConsensus>(slow_target, ts(0), ts(56 * 86400));
    assert_eq!(nbits_clamped, CompactTarget::new(GENESIS_NBITS));
    assert_eq!(target_clamped, GENESIS_TARGET);
}

#[zkvm_test]
fn boundary_header_retargets_once_per_epoch() {
    assert_boundary_retargets_once::<TestConsensus>();
    assert_boundary_retargets_once::<ShortEpoch>();
}

fn assert_boundary_retargets_once<C: Consensus>() {
    let _hash_guard = set_hash_success();
    let epoch_length = C::PARAMS.epoch_length;

    let mut state = test_state();
    state.height = epoch_length - 1;
    state.epoch_timestamp = ts(1_000_000);
    state.timestamps = [ts(1_000_600); WINDOW_SIZE];

    let (expected_nbits, ..) = calculate_next_target_required::<C>(
        target_from_bits(state.current_nbits),
        state.epoch_timestamp,
        state.timestamps[timestamp_slot(state.height)],
    );
    assert_ne!(
        expected_nbits, state.current_nbits,
        "test needs a boundary that actually changes nbits"
    );

    let header = HeaderInput {
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(1_000_700),
        nonce:       7,
    };
    state.apply_headers::<C>(&[header], &[]).expect("boundary header should apply");

    assert_eq!(state.height, epoch_length);
    assert_eq!(state.current_nbits, expected_nbits);
}

#[zkvm_test]
fn epoch_boundary_is_read_from_consensus_parameters() {
    assert_eq!(TestConsensus::PARAMS.epoch_length, 2016);
    assert_eq!(ShortEpoch::PARAMS.epoch_length, 32);

    let _hash_guard = set_hash_success();
    let mut state = test_state();
    state.epoch_timestamp = ts(1_000_000);
    state.timestamps = [ts(1_000_600); WINDOW_SIZE];
    state.height = ShortEpoch::PARAMS.epoch_length - 1;
    let starting_nbits = state.current_nbits;

    let header = HeaderInput {
        version:     1,
        merkle_root: [0x22; 32],
        timestamp:   ts(1_000_700),
        nonce:       7,
    };

    let mut short = state;
    short.apply_headers::<ShortEpoch>(&[header], &[]).expect("should apply");
    assert_ne!(
        short.current_nbits, starting_nbits,
        "ShortEpoch crossed a boundary and must retarget"
    );

    let mut main = state;
    main.apply_headers::<TestConsensus>(&[header], &[]).expect("should apply");
    assert_eq!(
        main.current_nbits, starting_nbits,
        "mainnet is mid-epoch here and must not retarget"
    );
}
