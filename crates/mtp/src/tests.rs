use zkpow_types::BlockTimestamp;

use crate::window::WINDOW_SIZE;
use crate::{Cold, Ring, Warm, Window, WindowState, errors::NotMedian, split_off_cold_headers};

fn cold_ring<const N: usize>(values: &[u32]) -> Ring<N, Cold> {
    let mut ring = Ring::<N, Cold>::new(BlockTimestamp::new(values[0]));
    for &value in &values[1..] {
        ring.push(BlockTimestamp::new(value));
    }
    ring
}

fn ring<const N: usize>(values: &[u32]) -> Ring<N> {
    let mut ring = cold_ring::<N>(&values[..N]).warm().unwrap();
    for &value in &values[N..] {
        ring.push(BlockTimestamp::new(value));
    }
    ring
}

fn naive_median(values: &[u32]) -> BlockTimestamp {
    let mut sorted = [0; 64];
    sorted[..values.len()].copy_from_slice(values);
    sorted[..values.len()].sort_unstable();
    BlockTimestamp::new(sorted[values.len() / 2])
}

#[test]
fn cold_window_sorts_the_height_derived_prefix() {
    for encoded in 0..3usize.pow(4) {
        let values = core::array::from_fn::<_, 4, _>(|i| (encoded / 3usize.pow(i as u32) % 3) as u32);
        for count in 1..=values.len() {
            let ring = cold_ring::<5>(&values[..count]);
            assert_eq!(ring.median(count as u32), naive_median(&values[..count]));
            assert!(ring.warm().is_none());
            let mut slots = [BlockTimestamp::new(u32::MAX); 5];
            for (slot, value) in slots.iter_mut().zip(&values[..count]) {
                *slot = BlockTimestamp::new(*value);
            }
            let restored = Ring::<5, Cold>::from_height(slots, count as u32 - 1);
            assert_eq!(restored.median(count as u32), naive_median(&values[..count]));
        }
    }
}

#[test]
fn ordered_median_matches_naive_across_the_cold_transition_and_wraps() {
    let mut cold = Window::<Cold>::new(BlockTimestamp::new(0));
    let mut held = [0; WINDOW_SIZE];
    for height in 0..WINDOW_SIZE {
        if height > 0 {
            cold.push(BlockTimestamp::new(height as u32));
            held[height] = height as u32;
        }
        assert_eq!(cold.median(height as u32 + 1), naive_median(&held[..=height]));
        assert_eq!(cold.warm().is_some(), height == WINDOW_SIZE - 1);
    }
    let mut ring = cold.warm().unwrap();
    for height in WINDOW_SIZE - 1..40 {
        if height >= WINDOW_SIZE {
            ring.push(BlockTimestamp::new(height as u32));
            held[height % WINDOW_SIZE] = height as u32;
        }
        assert_eq!(ring.median(), Some(naive_median(&held)));
        assert!(!ring.requires_median_hint());
        assert_eq!(Window::<Warm>::from_height(ring.values(), height as u32), ring);
    }
}

#[test]
fn median_holds_for_even_and_single_slot_windows() {
    for (median, expected) in [
        (ring::<5>(&[10, 20, 30, 40, 50]).median(), 30),
        (ring::<5>(&[10, 20, 30, 40, 50, 60, 70]).median(), 50),
        (ring::<4>(&[10, 20, 30, 40]).median(), 30),
        (ring::<1>(&[10, 20]).median(), 20),
    ] {
        assert_eq!(median, Some(BlockTimestamp::new(expected)));
    }
}

#[test]
fn decrease_requires_hints_until_a_full_ordered_run() {
    let mut ring = ring::<5>(&[10, 20, 30, 40, 50]);
    ring.push(BlockTimestamp::new(15));
    for value in [60, 70, 80] {
        assert_eq!(ring.median(), None);
        assert!(ring.requires_median_hint());
        ring.push(BlockTimestamp::new(value));
    }
    assert!(ring.requires_median_hint());
    ring.push(BlockTimestamp::new(90));
    assert_eq!(ring.median(), Some(BlockTimestamp::new(70)));
    assert!(!ring.requires_median_hint());
}

#[test]
fn unordered_windows_check_hints_by_rank() {
    let ring = ring::<11>(&[70, 10, 110, 30, 90, 50, 20, 100, 40, 80, 60]);
    assert!(ring.requires_median_hint());
    assert_eq!(ring.median_with_hint(&mut [].iter()), Err(NotMedian::MissingHint));
    for (value, expected) in [
        (60, Ok(BlockTimestamp::new(60))),
        (70, Err(NotMedian::TooNew)),
        (50, Err(NotMedian::TooOld)),
        (55, Err(NotMedian::TooOld)),
        (65, Err(NotMedian::TooNew)),
    ] {
        assert_eq!(ring.median_with_hint(&mut [BlockTimestamp::new(value)].iter()), expected);
    }
    let duplicates = self::ring::<5>(&[2, 0, 2, 3, 2]);
    assert_eq!(
        duplicates.median_with_hint(&mut [BlockTimestamp::new(2)].iter()),
        Ok(BlockTimestamp::new(2))
    );
}

#[test]
fn unordered_cold_history_requires_hints_immediately_after_warming() {
    let cold = cold_ring::<5>(&[2, 0, 2, 3, 2]);
    assert_eq!(cold.median(5), BlockTimestamp::new(2));
    let warm = cold.warm().unwrap();
    assert!(warm.requires_median_hint());
    assert_eq!(warm.median_with_hint(&mut [].iter()), Err(NotMedian::MissingHint));
    assert_eq!(
        warm.median_with_hint(&mut [BlockTimestamp::new(2)].iter()),
        Ok(BlockTimestamp::new(2))
    );
}

#[test]
fn zero_is_a_value_instead_of_an_empty_slot_sentinel() {
    assert_eq!(cold_ring::<5>(&[0, 10, 20]).median(3), BlockTimestamp::new(10));
    assert_eq!(ring::<5>(&[0, 0, 0, 0, 0]).median(), Some(BlockTimestamp::new(0)));
}

#[test]
fn chronological_history_preserves_absolute_slot_alignment() {
    for start in 0..30 {
        let history = core::array::from_fn::<_, WINDOW_SIZE, _>(|i| BlockTimestamp::new((i * 17 % 13) as u32));
        let WindowState::Warm(mut ring) = WindowState::from_timestamps(start, history).unwrap() else {
            panic!()
        };
        let height = start + WINDOW_SIZE as u32 - 1;
        assert_eq!(Window::<Warm>::from_height(ring.values(), height), ring);
        assert_eq!(ring.newest(), history[WINDOW_SIZE - 1]);
        ring.push(BlockTimestamp::new(60));
        assert_eq!(Window::<Warm>::from_height(ring.values(), height + 1), ring);
    }
    assert_eq!(WindowState::from_timestamps(0, []), None);
}

#[cfg(feature = "sort")]
#[test]
fn sorted_fallback_matches_naive_for_every_rotation() {
    let mut ring = ring::<WINDOW_SIZE>(&[0; WINDOW_SIZE]);
    let mut held = [0; WINDOW_SIZE];
    for height in WINDOW_SIZE..100 {
        let value = (height * 17 % 13) as u32;
        ring.push(BlockTimestamp::new(value));
        held[height % WINDOW_SIZE] = value;
        assert_eq!(ring.median_by_sort(), naive_median(&held));
    }
}

#[test]
fn cold_split_error_reports_required_and_actual_counts() {
    let error = split_off_cold_headers(&[0u8; 9]).unwrap_err();
    assert_eq!(error.required(), 10);
    assert_eq!(error.actual(), 9);
    assert_eq!(split_off_cold_headers(&[0u8; 12]).unwrap().1.len(), 2);
}

#[test]
#[should_panic(expected = "nonzero capacity")]
fn zero_capacity_is_rejected_at_construction() {
    let _ = Ring::<0, Cold>::new(BlockTimestamp::new(0));
}

#[test]
#[should_panic(expected = "warm ring requires a full window")]
fn warm_reconstruction_rejects_a_cold_height() {
    let _ = Window::<Warm>::from_height([BlockTimestamp::new(0); WINDOW_SIZE], 0);
}
