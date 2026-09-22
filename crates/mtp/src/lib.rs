//! Median time past over a fixed-size ring of block timestamps.
//!
//! The window size lives here and nowhere else. Callers name [`Window`], never `11`, and reach
//! anything derived from the size through a method on the ring.

#![no_std]

#[cfg(test)]
mod tests;

use core::fmt;

const WINDOW_SIZE: usize = 11;

/// The consensus timestamp window.
pub type Window = Ring<WINDOW_SIZE>;

/// Why a claimed median is not the window's median.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotMedian {
    TooNew,
    TooOld,
}

impl fmt::Display for NotMedian {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooNew => "claimed median is newer than half of the known window",
            Self::TooOld => "claimed median is older than half of the known window",
        })
    }
}

/// The last `N` timestamps, oldest overwritten first, carrying the length of the nondecreasing
/// run that ends at the newest value.
///
/// An unwritten slot is zero, so a ring that has not wrapped yet knows how many values it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ring<const N: usize> {
    values:      [u32; N],
    counter:     u32,
    ordered_run: u32,
    last_value:  u32,
}

impl<const N: usize> Ring<N> {
    const WINDOW_SMALL_HALF_SIZE: usize = N / 2;
    /// Slot of the median of a full window, indexed by `counter`. `counter` is the oldest slot,
    /// so the median sits `WINDOW_SMALL_HALF_SIZE` newer than it.
    const NEXT_MEDIAN_INDEX: [usize; N] = {
        let mut table = [0; N];
        let mut i = 0;
        while i < N {
            table[i] = (i + Self::WINDOW_SMALL_HALF_SIZE) % N;
            i += 1;
        }
        table
    };

    #[must_use]
    pub const fn new() -> Self {
        Self {
            values:      [0; N],
            counter:     0,
            ordered_run: 0,
            last_value:  0,
        }
    }

    /// Rebuild a ring from its slots and next write position. The ordered run is recomputed by
    /// replaying the values oldest first.
    #[must_use]
    pub fn new_from_values(values: [u32; N], counter: u32) -> Self {
        let counter = counter % N as u32;
        let mut ring = Self::new();
        for offset in 0..N {
            let value = values[(counter as usize + offset) % N];
            if value == 0 {
                continue;
            }
            ring.push(value);
        }

        ring.values = values;
        ring.counter = counter;
        ring
    }

    pub fn push(&mut self, value: u32) {
        if value >= self.last_value {
            self.ordered_run = (self.ordered_run + 1).min(N as u32);
        } else {
            self.ordered_run = 1;
        }
        self.last_value = value;

        self.values[self.counter as usize] = value;
        self.counter += 1;
        if self.counter as usize >= N {
            self.counter = 0;
        }
    }

    /// The median of the values held, valid only while [`Ring::is_ordered`].
    #[must_use]
    pub fn median(&self) -> u32 {
        debug_assert!(self.len() > 0 && self.is_ordered());
        self.values[self.median_index()]
    }

    /// Whether `claimed_median` is the median of the values held, by rank. This is the answer
    /// [`Ring::median`] cannot give once the window is out of order.
    pub fn is_value_median(&self, claimed_median: u32) -> Result<(), NotMedian> {
        let (less_count, equal_count) = {
            let mut counts: [usize; 3] = [0, 0, 0];
            for value in self.values {
                if value == 0 {
                    continue;
                }
                counts[((value.cmp(&claimed_median) as i8) + 1) as usize] += 1;
            }
            (counts[0], counts[1])
        };

        let half = self.len() / 2;
        if less_count > half {
            return Err(NotMedian::TooNew);
        }
        if less_count + equal_count <= half {
            return Err(NotMedian::TooOld);
        }
        Ok(())
    }

    /// Whether the values arrived in nondecreasing order, which is what lets [`Ring::median`]
    /// read the median off a known slot instead of ranking the window.
    #[must_use]
    pub fn is_ordered(&self) -> bool {
        self.ordered_run as usize >= self.len()
    }

    #[must_use]
    pub fn is_full(&self) -> bool {
        self.values[self.counter as usize] != 0
    }

    #[must_use]
    pub const fn capacity() -> usize {
        N
    }

    /// The slots and the next write position, for callers that persist a ring.
    #[must_use]
    pub fn parts(&self) -> (&[u32; N], u32) {
        (&self.values, self.counter)
    }

    fn len(&self) -> usize {
        if self.is_full() { N } else { self.counter as usize }
    }

    fn median_index(&self) -> usize {
        if self.is_full() {
            Self::NEXT_MEDIAN_INDEX[self.counter as usize]
        } else {
            self.counter as usize / 2
        }
    }
}

impl<const N: usize> Default for Ring<N> {
    fn default() -> Self {
        Self::new()
    }
}
