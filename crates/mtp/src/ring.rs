use core::{cmp::Ordering, marker::PhantomData};

use crate::{
    WINDOW_SIZE,
    errors::{InvalidParts, NotMedian},
};

/// The consensus timestamp window.
pub type Window<F = Cold> = Ring<WINDOW_SIZE, F>;

/// The ring holds fewer timestamps than its capacity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cold;

/// Every slot in the ring holds a timestamp.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Warm;

/// The last `N` timestamps, oldest overwritten first. `ordered_run` is the length of the
/// nondecreasing run ending at the newest value, capped at `N`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ring<const N: usize, F = Cold> {
    values:      [u32; N],
    next:        usize,
    len:         usize,
    ordered_run: usize,
    last_value:  u32,
    fill:        PhantomData<F>,
}

impl<const N: usize, F> Ring<N, F> {
    fn push_value(&mut self, value: u32) {
        if self.len == 0 || value >= self.last_value {
            if self.ordered_run < N {
                self.ordered_run += 1;
            }
        } else {
            self.ordered_run = 1;
        }
        self.last_value = value;

        self.values[self.next] = value;
        self.next += 1;
        if self.next == N {
            self.next = 0;
        }
        if self.len < N {
            self.len += 1;
        }
    }

    /// The number of timestamps currently held.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the ring contains no timestamps.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the backing slots, next write position, and number of values held.
    #[must_use]
    pub const fn parts(&self) -> (&[u32; N], usize, usize) {
        (&self.values, self.next, self.len)
    }
}

impl<const N: usize> Ring<N, Cold> {
    /// Inserts a timestamp.
    ///
    /// # Panics
    ///
    /// Panics when the ring is already full; call [`Ring::warm`] instead.
    pub fn push(&mut self, value: u32) {
        assert!(self.len < N, "a full cold ring must be warmed before pushing");
        self.push_value(value);
    }

    /// Creates an empty ring.
    ///
    /// # Panics
    ///
    /// Panics when `N` is zero.
    #[must_use]
    pub const fn new() -> Self {
        assert!(N > 0, "a timestamp ring must have nonzero capacity");
        Self {
            values:      [0; N],
            next:        0,
            len:         0,
            ordered_run: 0,
            last_value:  0,
            fill:        PhantomData,
        }
    }

    /// Rebuilds a cold ring from its first `len` slots in chronological order.
    pub fn from_parts(values: [u32; N], len: usize) -> Result<Self, InvalidParts> {
        if len >= N {
            return Err(InvalidParts::NotCold);
        }
        let (ordered_run, last_value) = ordered_run(values[..len].iter());
        Ok(Self {
            values,
            next: len,
            len,
            ordered_run,
            last_value,
            fill: PhantomData,
        })
    }

    /// The upper median, or `None` while the ring is empty or holds a decrease.
    #[must_use]
    pub fn median(&self) -> Option<u32> {
        (self.len > 0 && self.ordered_run >= self.len).then(|| self.values[self.len / 2])
    }

    /// The upper median by sorting a copy of the values held, or `None` while empty.
    #[must_use]
    pub fn median_by_sort(&self) -> Option<u32> {
        let mut sorted = self.values;
        sorted[..self.len].sort_unstable();
        (self.len > 0).then(|| sorted[self.len / 2])
    }

    /// Checks a claimed median by rank.
    pub fn is_value_median(&self, claimed_median: u32) -> Result<(), NotMedian> {
        rank_median(&self.values[..self.len], self.len / 2, claimed_median)
    }

    /// Converts to a warm ring once every slot holds a timestamp.
    pub fn warm(self) -> Result<Ring<N, Warm>, InvalidParts> {
        if self.len != N {
            return Err(InvalidParts::NotWarm);
        }
        Ok(Ring {
            values:      self.values,
            next:        self.next,
            len:         N,
            ordered_run: self.ordered_run,
            last_value:  self.last_value,
            fill:        PhantomData,
        })
    }
}

impl<const N: usize> Default for Ring<N, Cold> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Ring<N, Warm> {
    const WINDOW_SMALL_HALF_SIZE: usize = N / 2;

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

    /// Rebuilds a warm ring from its slots and next write position, which is also the oldest slot.
    pub fn from_parts(values: [u32; N], next: usize, len: usize) -> Result<Self, InvalidParts> {
        if len != N {
            return Err(InvalidParts::NotWarm);
        }
        if next >= N {
            return Err(InvalidParts::NextSlotOutOfBounds);
        }
        let (ordered_run, last_value) = ordered_run(values[next..].iter().chain(&values[..next]));
        Ok(Self {
            values,
            next,
            len: N,
            ordered_run,
            last_value,
            fill: PhantomData,
        })
    }

    /// Inserts a timestamp, overwriting the oldest slot.
    pub fn push(&mut self, value: u32) {
        self.push_value(value);
    }

    /// The most recently pushed timestamp.
    #[must_use]
    pub const fn newest(&self) -> u32 {
        self.last_value
    }

    /// The upper median in constant time, or `None` while the window holds a decrease.
    #[must_use]
    pub fn median(&self) -> Option<u32> {
        (self.ordered_run >= N).then(|| self.values[Self::MEDIAN_SLOT[self.next]])
    }

    /// Checks a claimed median by rank.
    pub fn is_value_median(&self, claimed_median: u32) -> Result<(), NotMedian> {
        rank_median(&self.values, Self::WINDOW_SMALL_HALF_SIZE, claimed_median)
    }
}

fn ordered_run<'a>(values: impl Iterator<Item = &'a u32>) -> (usize, u32) {
    let (mut run, mut last) = (0, 0);
    for &value in values {
        run = if run == 0 || value >= last { run + 1 } else { 1 };
        last = value;
    }
    (run, last)
}

fn rank_median(values: &[u32], half: usize, claimed_median: u32) -> Result<(), NotMedian> {
    let (mut less, mut equal) = (0, 0);
    for value in values {
        match value.cmp(&claimed_median) {
            Ordering::Less => less += 1,
            Ordering::Equal => equal += 1,
            Ordering::Greater => {},
        }
    }
    if less > half {
        return Err(NotMedian::TooNew);
    }
    if less + equal <= half {
        return Err(NotMedian::TooOld);
    }
    Ok(())
}
