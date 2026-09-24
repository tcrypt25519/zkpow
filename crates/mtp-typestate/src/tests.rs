use crate::{Cold, InvalidParts, NotMedian, Ring, Warm, Window, split_off_cold_headers};

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
    assert_eq!(Ring::<5, Warm>::from_parts([0; 5], 5, 5), Err(InvalidParts::NextSlotOutOfBounds));
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
