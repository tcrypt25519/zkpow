use core::{cmp::Ordering, marker::PhantomData};

use zkpow_types::BlockTimestamp;

use crate::errors::NotMedian;

pub type RingValues<const N: usize> = [BlockTimestamp; N];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cold;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Warm;

/// The last `N` timestamps, oldest overwritten first. `ordered_run` is the length of the
/// nondecreasing run ending at the newest value, capped at `N`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ring<const N: usize, F = Warm> {
    values:      RingValues<N>,
    next:        usize,
    ordered_run: usize,
    last_value:  BlockTimestamp,
    fill:        PhantomData<F>,
}

impl<const N: usize, F> Ring<N, F> {
    fn rebuild(values: RingValues<N>, current: usize, count: usize) -> Self {
        let last_value = values[current];
        let mut newer = last_value;
        let mut ordered_run = 0;
        for value in RingValuesIter::new(&values, current).take(count) {
            if value > newer {
                break;
            }
            ordered_run += 1;
            newer = value;
        }
        Self {
            values,
            next: if current == N - 1 { 0 } else { current + 1 },
            ordered_run,
            last_value,
            fill: PhantomData,
        }
    }

    /// Inserts a timestamp, overwriting the oldest slot.
    pub fn push(&mut self, value: BlockTimestamp) {
        self.ordered_run = if value >= self.last_value {
            (self.ordered_run + 1).min(N)
        } else {
            1
        };
        self.last_value = value;
        self.values[self.next] = value;
        self.next += 1;
        if self.next == N {
            self.next = 0;
        }
    }

    #[must_use]
    pub const fn values(&self) -> RingValues<N> {
        self.values
    }

    /// The most recently pushed timestamp.
    #[must_use]
    pub const fn newest(&self) -> BlockTimestamp {
        self.last_value
    }
}

impl<const N: usize> Ring<N, Cold> {
    /// Creates a ring seeded with the genesis timestamp.
    ///
    /// # Panics
    ///
    /// Panics when `N` is zero.
    #[must_use]
    pub fn new(genesis: BlockTimestamp) -> Self {
        assert!(N > 0, "a timestamp ring must have nonzero capacity");
        let mut values = [BlockTimestamp::new(0); N];
        values[0] = genesis;
        Self::from_height(values, 0)
    }

    pub fn from_height(values: RingValues<N>, height: u32) -> Self {
        assert!(N > 0, "a timestamp ring must have nonzero capacity");
        assert!((height as usize) < N, "a cold ring requires a cold height");
        Self::rebuild(values, height as usize, height as usize + 1)
    }

    /// The Cold ring always initializes with at least one value (genesis), and it always terminates
    /// at the end of the first cycle through the ring, so it's always the case that 0 < `next` < `N`
    /// while the ring is warming. A `next` value of 0 indicates that the cycle has been completed,
    /// the ring has wrapped around, and is ready to transition to the Warm state.
    #[must_use]
    fn len(&self) -> usize {
        if self.next == 0 { N } else { self.next }
    }

    #[must_use]
    pub fn median(&self) -> BlockTimestamp {
        let count = self.len();
        let mut sorted = self.values;
        sorted[..count].sort_unstable();
        sorted[count / 2]
    }

    pub fn take_cold_headers<I: Iterator>(&self, headers: I) -> core::iter::Take<I> {
        headers.take(N - self.len())
    }

    pub fn warm(self) -> Option<Ring<N, Warm>> {
        (self.next == 0).then_some(Ring {
            values:      self.values,
            next:        self.next,
            ordered_run: self.ordered_run,
            last_value:  self.last_value,
            fill:        PhantomData,
        })
    }
}

impl<const N: usize> Ring<N, Warm> {
    /// Slot of the median indexed by the oldest slot.
    const MEDIAN_SLOT: [usize; N] = {
        let mut table = [0; N];
        let mut i = 0;
        while i < N {
            table[i] = (i + N / 2) % N;
            i += 1;
        }
        table
    };

    pub fn from_height(values: RingValues<N>, height: u32) -> Self {
        assert!(N > 0, "a timestamp ring must have nonzero capacity");
        assert!(u64::from(height) + 1 >= N as u64, "a warm ring requires a full window");
        Self::rebuild(values, height as usize % N, N)
    }

    #[must_use]
    pub fn median(&self) -> Option<BlockTimestamp> {
        (self.ordered_run >= N).then(|| self.values[Self::MEDIAN_SLOT[self.next]])
    }

    pub fn median_with_hint<'a>(
        &self,
        hints: &mut impl Iterator<Item = &'a BlockTimestamp>,
    ) -> Result<BlockTimestamp, NotMedian> {
        if let Some(median) = self.median() {
            return Ok(median);
        }
        let median = *hints.next().ok_or(NotMedian::MissingHint)?;
        self.is_value_median(median)?;
        Ok(median)
    }

    /// The upper median by sorting a copy of the values held.
    #[cfg(feature = "sort")]
    #[must_use]
    pub fn median_by_sort(&self) -> BlockTimestamp {
        self.median().unwrap_or_else(|| {
            let mut sorted = self.values;
            sorted.sort_unstable();
            sorted[N / 2]
        })
    }

    #[must_use]
    pub const fn requires_median_hint(&self) -> bool {
        self.ordered_run < N
    }

    /// Checks a claimed median by rank.
    fn is_value_median(&self, claimed_median: BlockTimestamp) -> Result<(), NotMedian> {
        let (mut less, mut equal) = (0, 0);
        for value in &self.values {
            match value.cmp(&claimed_median) {
                Ordering::Less => less += 1,
                Ordering::Equal => equal += 1,
                Ordering::Greater => {},
            }
        }
        if less > N / 2 {
            return Err(NotMedian::TooNew);
        }
        if less + equal <= N / 2 {
            return Err(NotMedian::TooOld);
        }
        Ok(())
    }
}

// Iterates over the values in reverse order, emitting each one exactly once.
// Starting from the given value it steps backwards through the ring until it's
// at the oldest edge, before wrapping around to the newest value and continuing
// until it's emitted the value just before the starting slot.
struct RingValuesIter<'a, const N: usize> {
    values: &'a RingValues<N>,
    index:  usize,
    start:  usize,
}

impl<'a, const N: usize> RingValuesIter<'a, N> {
    fn new(values: &'a RingValues<N>, start: usize) -> Self {
        Self {
            values,
            index: start,
            start,
        }
    }
}

impl<const N: usize> Iterator for RingValuesIter<'_, N> {
    type Item = BlockTimestamp;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= N {
            return None;
        }
        let value = self.values[self.index];
        self.index = if self.index == 0 { N - 1 } else { self.index - 1 };
        if self.index == self.start {
            self.index = N;
        }
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::{WINDOW_SIZE, Window};

    #[test]
    fn reverse_iterator_visits_each_slot_once() {
        let values = [0, 1, 2, 3, 4].map(BlockTimestamp::new);
        for start in 0..values.len() {
            let mut iter = RingValuesIter::new(&values, start);
            for offset in 0..values.len() {
                assert_eq!(iter.next(), Some(values[(start + values.len() - offset) % values.len()]));
            }
            assert_eq!(iter.next(), None);
            assert_eq!(iter.next(), None);
        }
        let values = [BlockTimestamp::new(7)];
        let mut iter = RingValuesIter::new(&values, 0);
        assert_eq!(iter.next(), Some(values[0]));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn reconstruction_counts_only_the_ordered_suffix_at_every_rotation() {
        for encoded in 0..3usize.pow(5) {
            let chronological =
                core::array::from_fn::<_, 5, _>(|i| BlockTimestamp::new((encoded / 3usize.pow(i as u32) % 3) as u32));
            let expected_run = 1 + chronological.windows(2).rev().take_while(|pair| pair[0] <= pair[1]).count();
            for height in 4..19 {
                let current = height as usize % 5;
                let mut values = [BlockTimestamp::new(0); 5];
                for (offset, value) in chronological.iter().rev().enumerate() {
                    values[(current + 5 - offset) % 5] = *value;
                }
                let mut ring = Ring::<5>::from_height(values, height);
                assert_eq!(ring.ordered_run, expected_run);
                assert_eq!(ring.newest(), chronological[4]);
                assert_eq!(ring.median(), (expected_run == 5).then_some(chronological[2]));
                ring.push(BlockTimestamp::new(3));
                assert_eq!(ring.newest(), BlockTimestamp::new(3));
                assert_eq!(ring.values[(current + 1) % 5], BlockTimestamp::new(3));
                assert_eq!(ring.median().is_some(), expected_run >= 4);
            }
        }
    }

    #[test]
    fn reconstruction_handles_single_slot_and_maximum_height() {
        let value = BlockTimestamp::new(7);
        let ring = Ring::<1>::from_height([value], u32::MAX);
        assert_eq!(ring.median(), Some(value));
        assert_eq!(ring.ordered_run, 1);
        let ring = Window::<Warm>::from_height([value; WINDOW_SIZE], u32::MAX);
        assert_eq!(ring.median(), Some(value));
        assert_eq!(ring.next, ((u64::from(u32::MAX) + 1) % WINDOW_SIZE as u64) as usize);
    }
}
