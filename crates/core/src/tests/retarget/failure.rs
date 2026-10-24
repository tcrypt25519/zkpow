use zkvm_test::zkvm_test;

use crate::{calculate_next_work_required, tests::*, CompactTarget, Target};

#[zkvm_test]
fn retarget_clamps_negative_and_zero_timespans() {
    let old_target = Target::from_limbs([0, 0, 0, 0x0000_0000_7FFF_0000]);

    // Negative timespan (blocks went backward in epoch duration)
    let (nbits_neg, target_neg, work_neg) =
        calculate_next_work_required::<TestConsensus>(old_target, ts(1_000_000), ts(900_000));
    // Zero timespan (same timestamp)
    let (nbits_zero, target_zero, work_zero) =
        calculate_next_work_required::<TestConsensus>(old_target, ts(1_000_000), ts(1_000_000));
    // 0.25x lower clamp threshold (302,400s)
    let (nbits_quarter, target_quarter, work_quarter) =
        calculate_next_work_required::<TestConsensus>(old_target, ts(0), ts(302_400));

    assert_eq!(target_neg, target_quarter, "negative timespan must clamp to 0.25x");
    assert_eq!(target_zero, target_quarter, "zero timespan must clamp to 0.25x");
    assert_eq!(nbits_neg, nbits_quarter);
    assert_eq!(nbits_zero, nbits_quarter);
    assert_eq!(work_neg, work_quarter);
    assert_eq!(work_zero, work_quarter);
}

#[zkvm_test]
fn retarget_clamps_to_pow_limit_on_extreme_slowdown() {
    // Starting already at genesis pow_limit
    let genesis_target = GENESIS_TARGET;
    // Epoch took 1000 days (huge timespan)
    let (nbits, target, _) = calculate_next_work_required::<TestConsensus>(genesis_target, ts(0), ts(1000 * 86400));

    // Must clamp to pow_limit (GENESIS_TARGET / GENESIS_NBITS)
    assert_eq!(nbits, CompactTarget::new(GENESIS_NBITS));
    assert_eq!(target, GENESIS_TARGET);
}
