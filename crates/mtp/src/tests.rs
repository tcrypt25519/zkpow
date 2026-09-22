use crate::{NotMedian, Ring, WINDOW_SIZE, Window};

/// The median the consensus rules describe: sort the values held and take the upper middle one.
fn naive_median(chronological: &[u32]) -> u32 {
    let mut sorted = [0u32; 64];
    let len = chronological.len();
    sorted[..len].copy_from_slice(chronological);
    sorted[..len].sort_unstable();
    sorted[len / 2]
}

fn pushed<const N: usize>(values: &[u32]) -> Ring<N> {
    let mut ring = Ring::new();
    for value in values {
        ring.push(*value);
    }
    ring
}

#[test]
fn median_table_rotates_by_the_small_half() {
    assert_eq!(Window::NEXT_MEDIAN_INDEX, [5, 6, 7, 8, 9, 10, 0, 1, 2, 3, 4]);
    assert_eq!(Ring::<4>::NEXT_MEDIAN_INDEX, [2, 3, 0, 1]);
}

#[test]
fn median_matches_the_naive_median_while_filling() {
    let mut ring = Window::new();
    let mut seen = [0u32; WINDOW_SIZE];

    for (index, value) in (1..=WINDOW_SIZE as u32).map(|i| i * 10).enumerate() {
        ring.push(value);
        seen[index] = value;
        assert_eq!(ring.median(), naive_median(&seen[..=index]), "height {index}");
    }
    assert!(ring.is_full());
}

#[test]
fn median_matches_the_naive_median_across_wraps() {
    let mut ring = Window::new();
    let mut seen = [0u32; WINDOW_SIZE];

    for height in 0..WINDOW_SIZE as u32 * 3 {
        let value = 1_700_000_000 + height * 600;
        ring.push(value);
        seen[height as usize % WINDOW_SIZE] = value;

        let expected = if height as usize >= WINDOW_SIZE {
            naive_median(&seen)
        } else {
            naive_median(&seen[..=height as usize])
        };
        assert_eq!(ring.median(), expected, "height {height}");
    }
}

#[test]
fn median_holds_for_other_window_sizes() {
    assert_eq!(pushed::<5>(&[10, 20, 30, 40, 50]).median(), 30);
    assert_eq!(pushed::<5>(&[10, 20, 30, 40, 50, 60, 70]).median(), 50);
    assert_eq!(pushed::<4>(&[10, 20, 30, 40]).median(), 30);
    assert_eq!(pushed::<1>(&[10, 20]).median(), 20);
}

#[test]
fn equal_timestamps_keep_the_window_ordered() {
    let ring = pushed::<WINDOW_SIZE>(&[1, 2, 3, 4, 5, 6, 6, 6, 7, 8, 9]);
    assert!(ring.is_ordered());
    assert_eq!(ring.median(), 6);
    assert_eq!(ring.is_value_median(6), Ok(()));
}

#[test]
fn a_decrease_resets_the_ordered_run() {
    let mut ring = pushed::<WINDOW_SIZE>(&[10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110]);
    assert!(ring.is_ordered());

    ring.push(15);
    assert!(!ring.is_ordered());

    // The run has to cover the whole window again before the median is readable.
    for value in 120..120 + WINDOW_SIZE as u32 - 2 {
        ring.push(value);
        assert!(!ring.is_ordered(), "run too short at {value}");
    }
    ring.push(200);
    assert!(ring.is_ordered());
}

#[test]
fn a_partial_window_is_ordered_only_for_the_values_it_holds() {
    let mut ring = Window::new();
    ring.push(30);
    ring.push(20);
    assert!(!ring.is_ordered());
    ring.push(40);
    assert!(!ring.is_ordered());

    let ordered = pushed::<WINDOW_SIZE>(&[30, 40]);
    assert!(ordered.is_ordered());
    assert_eq!(ordered.median(), 40);
}

#[test]
fn is_value_median_accepts_the_median_of_an_unordered_window() {
    let ring = pushed::<WINDOW_SIZE>(&[70, 10, 110, 30, 90, 50, 20, 100, 40, 80, 60]);
    assert!(!ring.is_ordered());
    assert_eq!(ring.is_value_median(60), Ok(()));
}

#[test]
fn is_value_median_rejects_values_off_the_median() {
    let ring = pushed::<WINDOW_SIZE>(&[70, 10, 110, 30, 90, 50, 20, 100, 40, 80, 60]);
    assert_eq!(ring.is_value_median(70), Err(NotMedian::TooNew));
    assert_eq!(ring.is_value_median(50), Err(NotMedian::TooOld));
    assert_eq!(ring.is_value_median(55), Err(NotMedian::TooOld));
    assert_eq!(ring.is_value_median(65), Err(NotMedian::TooNew));
    assert_eq!(ring.is_value_median(u32::MAX), Err(NotMedian::TooNew));
    assert_eq!(ring.is_value_median(0), Err(NotMedian::TooOld));
}

#[test]
fn is_value_median_ignores_empty_slots() {
    let ring = pushed::<WINDOW_SIZE>(&[30, 10, 20]);
    assert_eq!(ring.is_value_median(20), Ok(()));
    assert_eq!(ring.is_value_median(10), Err(NotMedian::TooOld));
    assert_eq!(ring.is_value_median(30), Err(NotMedian::TooNew));
}

#[test]
fn new_from_values_round_trips_a_wrapped_ring() {
    let ring = pushed::<WINDOW_SIZE>(&[
        10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140,
    ]);
    let (values, counter) = ring.parts();
    let rebuilt = Window::new_from_values(*values, counter);

    assert_eq!(rebuilt, ring);
    assert_eq!(rebuilt.median(), ring.median());
    assert!(rebuilt.is_ordered());
}

#[test]
fn new_from_values_recovers_the_run_of_an_unordered_ring() {
    let ring = pushed::<WINDOW_SIZE>(&[10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 15, 120]);
    let (values, counter) = ring.parts();
    let rebuilt = Window::new_from_values(*values, counter);

    assert_eq!(rebuilt, ring);
    assert!(!rebuilt.is_ordered());
    // Window is 30..=110, 15, 120; the 15 sinks to the bottom, so the median is 70.
    assert_eq!(rebuilt.is_value_median(70), Ok(()));
}

#[test]
fn new_from_values_recovers_a_partial_window() {
    let ring = pushed::<WINDOW_SIZE>(&[10, 20, 30]);
    let (values, counter) = ring.parts();
    let rebuilt = Window::new_from_values(*values, counter);

    assert_eq!(rebuilt, ring);
    assert!(!rebuilt.is_full());
    assert_eq!(rebuilt.median(), 20);
}

#[test]
fn new_from_values_normalizes_a_counter_past_the_end() {
    let values = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110];
    let rebuilt = Window::new_from_values(values, WINDOW_SIZE as u32);

    assert_eq!(rebuilt, Window::new_from_values(values, 0));
    assert_eq!(rebuilt.median(), 60);
}

#[test]
fn an_empty_ring_holds_nothing() {
    let ring = Window::new();
    assert!(!ring.is_full());
    assert_eq!(ring.parts(), (&[0; WINDOW_SIZE], 0));
    assert_eq!(Window::capacity(), WINDOW_SIZE);
}
