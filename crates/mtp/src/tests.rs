use crate::{
    Cold, Ring, WINDOW_LARGE_HALF_SIZE, WINDOW_MAX_INDEX, WINDOW_SIZE, WINDOW_SMALL_HALF_SIZE, Warm, Window,
    advance_ordered_timestamp_run,
    errors::{InvalidParts, NotMedian},
    has_ordered_timestamp_window, ordered_median_timestamp_slot, ordered_timestamp_run, requires_median_hint,
    split_off_cold_headers, timestamp_count, timestamp_slot,
};

fn naive_median(values: &[u32]) -> u32 {
    let mut sorted = [0u32; 64];
    sorted[..values.len()].copy_from_slice(values);
    sorted[..values.len()].sort_unstable();
    sorted[values.len() / 2]
}

fn cold<const N: usize>(values: &[u32]) -> Ring<N, Cold> {
    let mut ring = Ring::<N>::new();
    for &value in values {
        ring.push(value);
    }
    ring
}

fn warm<const N: usize>(values: &[u32]) -> Ring<N, Warm> {
    let (fill, rest) = values.split_at(N);
    let mut ring = cold(fill).warm().expect("values must fill the ring");
    for &value in rest {
        ring.push(value);
    }
    ring
}

#[test]
fn empty_ring_has_no_median() {
    let ring = Window::new();
    assert!(ring.is_empty());
    assert_eq!(ring.median(), None);
    assert_eq!(ring.median_by_sort(), None);
    assert_eq!(ring.parts(), (&[0; 11], 0, 0));
}

#[test]
fn ordered_median_matches_naive_while_filling_and_across_wraps() {
    let mut ring = Window::new();
    let mut held = [0u32; 11];
    let value = |height: usize| 1_700_000_000 + height as u32 * 600;

    for height in 0..11usize {
        ring.push(value(height));
        held[height] = value(height);
        assert_eq!(ring.median(), Some(naive_median(&held[..=height])), "cold height {height}");
    }

    let mut ring = ring.warm().unwrap();
    for height in 11..33usize {
        ring.push(value(height));
        held[height % 11] = value(height);
        assert_eq!(ring.median(), Some(naive_median(&held)), "warm height {height}");
    }
}

#[test]
fn median_holds_for_even_and_single_slot_windows() {
    assert_eq!(warm::<5>(&[10, 20, 30, 40, 50]).median(), Some(30));
    assert_eq!(warm::<5>(&[10, 20, 30, 40, 50, 60, 70]).median(), Some(50));
    assert_eq!(cold::<5>(&[10, 20, 30, 40]).median(), Some(30));
    assert_eq!(warm::<4>(&[10, 20, 30, 40]).median(), Some(30));
    assert_eq!(warm::<1>(&[10, 20]).median(), Some(20));
}

#[test]
fn decrease_removes_the_median_until_a_full_ordered_run() {
    let mut ring = warm::<5>(&[10, 20, 30, 40, 50]);
    ring.push(15);
    assert_eq!(ring.median(), None);
    for value in [60, 70, 80] {
        ring.push(value);
        assert_eq!(ring.median(), None, "ordered run is not long enough at {value}");
    }
    ring.push(90);
    assert_eq!(ring.median(), Some(70));
}

#[test]
fn cold_decrease_persists_into_the_warm_ring_until_overwritten() {
    let mut ring = cold::<5>(&[30, 20]);
    assert_eq!(ring.median(), None);
    assert_eq!(ring.median_by_sort(), Some(30));
    for value in [40, 50, 60] {
        ring.push(value);
        assert_eq!(ring.median(), None);
    }
    let mut ring = ring.warm().unwrap();
    assert_eq!(ring.median(), None);
    ring.push(70);
    assert_eq!(ring.median(), Some(50));
}

#[test]
fn rank_check_accepts_unordered_median_and_rejects_neighbors() {
    let ring = warm::<11>(&[70, 10, 110, 30, 90, 50, 20, 100, 40, 80, 60]);
    assert_eq!(ring.median(), None);
    assert_eq!(ring.is_value_median(60), Ok(()));
    assert_eq!(ring.is_value_median(70), Err(NotMedian::TooNew));
    assert_eq!(ring.is_value_median(50), Err(NotMedian::TooOld));
    assert_eq!(ring.is_value_median(55), Err(NotMedian::TooOld));
    assert_eq!(ring.is_value_median(65), Err(NotMedian::TooNew));
}

#[test]
fn cold_unordered_median_uses_only_values_held() {
    let ring = cold::<5>(&[30, 10, 20]);
    assert_eq!(ring.median(), None);
    assert_eq!(ring.median_by_sort(), Some(20));
    assert_eq!(ring.is_value_median(20), Ok(()));
    assert_eq!(ring.is_value_median(10), Err(NotMedian::TooOld));
    assert_eq!(ring.is_value_median(30), Err(NotMedian::TooNew));
}

#[test]
fn zero_is_a_value_instead_of_an_empty_slot_sentinel() {
    let ring = cold::<5>(&[0, 10, 20]);
    assert_eq!(ring.len(), 3);
    assert_eq!(ring.median(), Some(10));
    assert_eq!(ring.is_value_median(10), Ok(()));
}

#[test]
fn warm_parts_round_trip_ordered_and_unordered_rings() {
    let ordered = warm::<5>(&[10, 20, 30, 40, 50, 60, 70]);
    let (values, next, len) = ordered.parts();
    let rebuilt = Ring::<5, Warm>::from_parts(*values, next, len).unwrap();
    assert_eq!(rebuilt, ordered);
    assert_eq!(rebuilt.median(), Some(50));

    let unordered = warm::<5>(&[10, 20, 30, 40, 50, 15, 60]);
    let (values, next, len) = unordered.parts();
    let rebuilt = Ring::<5, Warm>::from_parts(*values, next, len).unwrap();
    assert_eq!(rebuilt, unordered);
    assert_eq!(rebuilt.median(), None);
    assert_eq!(rebuilt.is_value_median(40), Ok(()));
}

#[test]
fn cold_parts_round_trip_ordered_and_unordered_rings() {
    let ordered = cold::<5>(&[10, 20, 30]);
    let (values, _, len) = ordered.parts();
    let rebuilt = Ring::<5, Cold>::from_parts(*values, len).unwrap();
    assert_eq!(rebuilt, ordered);
    assert_eq!(rebuilt.median(), Some(20));

    let unordered = cold::<5>(&[10, 30, 20]);
    let (values, _, len) = unordered.parts();
    let rebuilt = Ring::<5, Cold>::from_parts(*values, len).unwrap();
    assert_eq!(rebuilt, unordered);
    assert_eq!(rebuilt.median(), None);
}

#[test]
fn invalid_parts_are_rejected() {
    assert_eq!(Ring::<5, Cold>::from_parts([0; 5], 5), Err(InvalidParts::NotCold));
    assert_eq!(Ring::<5, Warm>::from_parts([0; 5], 0, 4), Err(InvalidParts::NotWarm));
    assert_eq!(
        Ring::<5, Warm>::from_parts([0; 5], 5, 5),
        Err(InvalidParts::NextSlotOutOfBounds)
    );
    assert_eq!(cold::<5>(&[10, 20, 30, 40]).warm(), Err(InvalidParts::NotWarm));
}

#[test]
fn cold_split_error_reports_required_and_actual_counts() {
    let headers = [0u8; 9];
    let error = split_off_cold_headers(&headers).unwrap_err();
    assert_eq!(error.required(), 10);
    assert_eq!(error.actual(), 9);
    assert_eq!(split_off_cold_headers(&[0u8; 12]).unwrap().1.len(), 2);
}

#[test]
#[should_panic(expected = "must be warmed")]
fn full_cold_ring_rejects_a_push() {
    cold::<3>(&[1, 2, 3]).push(4);
}

#[test]
#[should_panic(expected = "nonzero capacity")]
fn zero_capacity_is_rejected_at_construction() {
    let _ = Ring::<0>::new();
}

#[test]
fn window_constants_match_expected() {
    assert_eq!(WINDOW_SIZE, 11);
    assert_eq!(WINDOW_SMALL_HALF_SIZE, 5);
    assert_eq!(WINDOW_LARGE_HALF_SIZE, 6);
    assert_eq!(WINDOW_MAX_INDEX, 10);
    assert_eq!(WINDOW_MAX_INDEX, WINDOW_SIZE - 1);
    assert_eq!(WINDOW_SMALL_HALF_SIZE, WINDOW_SIZE / 2);
    assert_eq!(WINDOW_LARGE_HALF_SIZE, (WINDOW_SIZE + 1) / 2);
}

#[test]
fn timestamp_slot_and_count_test() {
    assert_eq!(timestamp_slot(0), 0);
    assert_eq!(timestamp_slot(10), 10);
    assert_eq!(timestamp_slot(11), 0);
    assert_eq!(timestamp_slot(12), 1);

    assert_eq!(timestamp_count(0), 1);
    assert_eq!(timestamp_count(5), 6);
    assert_eq!(timestamp_count(10), 11);
    assert_eq!(timestamp_count(11), 11);
    assert_eq!(timestamp_count(100), 11);
}

#[test]
fn has_ordered_timestamp_window_checks_run_length() {
    assert!(!has_ordered_timestamp_window(0));
    assert!(!has_ordered_timestamp_window(9));
    assert!(has_ordered_timestamp_window(10));
    assert!(has_ordered_timestamp_window(11));
    assert!(has_ordered_timestamp_window(100));
}

#[test]
fn advance_ordered_timestamp_run_increments_and_resets() {
    let mut run = 0u64;
    run = advance_ordered_timestamp_run(run, 100, 100);
    assert_eq!(run, 1);
    run = advance_ordered_timestamp_run(run, 100, 101);
    assert_eq!(run, 2);
    run = advance_ordered_timestamp_run(run, 101, 100);
    assert_eq!(run, 0);
}

#[test]
fn ordered_timestamp_run_evaluates_window_order() {
    // 11 ascending values
    let mut timestamps = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110];
    assert_eq!(ordered_timestamp_run(&timestamps, 10), 10);

    // One dip at slot 5
    timestamps[5] = 5;
    // A push of 70 at slot 6 counts one ordered link (5 <= 70) before the
    // run breaks against slot 4's 50.
    assert_eq!(ordered_timestamp_run(&timestamps, 6), 1);
}

#[test]
fn ordered_median_timestamp_slot_calculation() {
    for height in 0..33u32 {
        let expected = ((height + 6) % 11) as usize;
        assert_eq!(ordered_median_timestamp_slot(height), expected);
    }
}

#[test]
fn requires_median_hint_checks_window_and_order() {
    // Before window is full: never requires hint
    assert!(!requires_median_hint(0, 0));
    assert!(!requires_median_hint(10, 0));
    assert!(!requires_median_hint(10, 10));

    // At or after full window:
    // When ordered run >= 10: does not require hint
    assert!(!requires_median_hint(11, 10));
    assert!(!requires_median_hint(100, 10));

    // When ordered run < 10: requires hint
    assert!(requires_median_hint(11, 9));
    assert!(requires_median_hint(100, 0));
}
