//! Median time past over a ring of block timestamps.
//!
//! A [`Ring`] is [`Cold`] while it fills and [`Warm`] once every slot holds a timestamp. The warm
//! ring reads its median off a compile-time slot table. Ordering is tracked at runtime, so
//! [`Ring::median`] is `None` while the window holds a chronological decrease.
//!
//! ```
//! use zkpow_mtp::Window;
//!
//! let mut ring = Window::new();
//! for value in 0..11 {
//!     ring.push(value);
//! }
//! let ring = ring.warm().unwrap();
//! assert_eq!(ring.median(), Some(5));
//! ```

#![no_std]

pub mod errors;
mod ring;

use errors::InsufficientColdHeaders;
pub use ring::{Cold, Ring, Warm, Window};

/// Number of previous block timestamps kept for median-time-past and ordered-window checks.
pub const WINDOW_SIZE: usize = 11;

/// Half the window, rounded down; the number of timestamps that may sit below the median.
pub const WINDOW_SMALL_HALF_SIZE: usize = WINDOW_SIZE / 2;

/// Half the window, rounded up; the number of timestamps that may sit above the median.
pub const WINDOW_LARGE_HALF_SIZE: usize = (WINDOW_SIZE + 1) / 2;

/// Maximum index in the sliding window.
pub const WINDOW_MAX_INDEX: usize = WINDOW_SIZE - 1;

#[cfg(test)]
mod tests;

/// Splits headers into the prefix that fills an empty consensus window and the warm remainder.
pub fn split_off_cold_headers<T>(headers: &[T]) -> Result<(&[T], &[T]), InsufficientColdHeaders> {
    let required = WINDOW_SIZE - 1;
    if headers.len() < required {
        return Err(InsufficientColdHeaders::new(headers.len()));
    }
    Ok(headers.split_at(required))
}

/// The window slot written by the block at `height`.
#[must_use]
pub const fn timestamp_slot(height: u32) -> usize {
    height as usize % WINDOW_SIZE
}

/// Slots filled once the block at `height` is recorded: `height + 1`, capped at the window size.
#[must_use]
pub const fn timestamp_count(height: u32) -> usize {
    let count = height as usize + 1;
    if count > WINDOW_SIZE { WINDOW_SIZE } else { count }
}

/// Whether the run of nondecreasing links orders the entire window, pinning the median to a known slot.
#[must_use]
pub const fn has_ordered_timestamp_window(ordered_timestamp_run: u64) -> bool {
    ordered_timestamp_run >= WINDOW_MAX_INDEX as u64
}

/// Extend the run of nondecreasing links by one, or reset it to zero on a decrease.
#[must_use]
pub const fn advance_ordered_timestamp_run(ordered_timestamp_run: u64, previous_timestamp: u32, timestamp: u32) -> u64 {
    if timestamp >= previous_timestamp {
        ordered_timestamp_run.saturating_add(1)
    } else {
        0
    }
}

/// Recount the nondecreasing links by walking the window backwards from the slot of `height`.
#[must_use]
pub fn ordered_timestamp_run<T: PartialOrd>(timestamps: &[T; WINDOW_SIZE], height: u32) -> u64 {
    let count = timestamp_count(height);
    let mut run = 0;
    let mut newer = &timestamps[timestamp_slot(height)];
    for offset in 1..count {
        let older = &timestamps[timestamp_slot(height + (WINDOW_SIZE - offset) as u32)];
        if older > newer {
            break;
        }
        run += 1;
        newer = older;
    }
    run
}

/// The slot holding the median while the window ending at `height` is fully ordered.
#[must_use]
pub const fn ordered_median_timestamp_slot(height: u32) -> usize {
    timestamp_slot(height + WINDOW_LARGE_HALF_SIZE as u32)
}

/// Whether the block at `height` needs a prover-supplied median: its window is full but not
/// provably ordered.
#[must_use]
pub const fn requires_median_hint(height: u64, previous_ordered_timestamp_run: u64) -> bool {
    height >= WINDOW_SIZE as u64 && !has_ordered_timestamp_window(previous_ordered_timestamp_run)
}
