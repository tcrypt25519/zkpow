//! # Median time past over a ring of block timestamps.
//!
//! A ring is a circular buffer of a fixed size `N`, where `N` is the size of the MTP lookback
//! window. Every timestamp required to calculate the MTP is in the ring; with one timestamp
//! entering and one leaving for each header. The circular buffer allows us to avoid shifting
//! timestamps as headers are added and removed. The index for the next slot to write to is
//! determined at that start of a run by the chain's height, and then for the remainder of the
//! run it simply increments at header, and when it becomes `N` it's reset back to 0.
//!
//! ## Ring Types
//!
//! We model two types of rings: `Cold` and `Warm`. A cold ring is simply a ring that is in its
//! first iteration and has yet to complete a full cycle. This is a notable property because it
//! means that the ring has fewer than `N` timestamps in it which is a necessary state when the
//! chain first begins. But if the can ever be less than full then we must always check at every
//! height, even though after the first ~2 hours of the network's life that state is no longer
//! possible. We enable the MTP logic to disregard this situation be splitting it into a special
//! window that must be completed in one go and then be over with, allowing all subsequent code
//! to operate with the knowledge that the window is always ull.
//!
//! Cold rings sort their height-derived prefix. Warm rings use every slot and track timestamp
//! order internally. Ordered windows read their median off a compile-time slot table; unordered
//! windows use a checked hint or, with the `sort` feature, a sorted copy.
//!
//! ```
//! use zkpow_mtp::{Cold, Window};
//!
//! let mut ring = Window::<Cold>::new(zkpow_types::BlockTimestamp::new(0));
//! for value in 1..11 {
//!     ring.push(zkpow_types::BlockTimestamp::new(value));
//! }
//! assert_eq!(ring.warm().unwrap().median(), Some(zkpow_types::BlockTimestamp::new(5)));
//! ```
//!
//! ## Ordered vs Unordered
//!
//! The standard method of calculating a median is to take the items, sort them, and then take
//! the middle index. The sorting is necessary because any individual timestamp is allowed to be
//! earlier than the last four timestamps. And in a given block, you can't know that you have a
//! sorted window, just because you have a valid window. On regular computers sorting 11 32-bit
//! integers is trivial enough every block, but on SP1 it was demonstrated during development
//! that it's significantly more expensive than most of the other logic the guest executes. To
//! avoid sorting, we have two alternative algorithms that we use on a block-by-block basis. We
//! track the number of timestamps we've seen in a row that do not go backwards. If this number
//! is `N` or greater then we know that the window is sorted. We did not have to do any sorting
//! ourselves. We just observed that the miners sorted the blocks for us. In this case, we can
//! just take the median index and know that that is our median time. It's the standard
//! algorithm. We just selectively apply it when the precondition has made itself true without
//! us having to do the work.
//!
//! The other situation is when there have been timestamps within the last `N` blocks that have
//! gone backwards. Now we know that at least one of those steps caused the ring to become
//! unsorted. To avoid causing ourselves more work than we're trying to reduce, we do not attempt
//! to track specifically where in the ring the poison time is.
//!
//! We simply keep track of how many non-decreasing timestamps we've seen since then. After `N`
//! non-decreasing timestamps, we know that all of the bad ones are flushed out. Until that
//! happens, we use an alternative strategy for checking the MTP. First, we rely on the host to
//! tell us what the median is for that block. This is an untrusted value. We don't know that
//! it's accurate, but we can check to see whether or not it would be an acceptable median if it
//! was genuinely the median. Then, we can prove that it is or is not the genuine median by
//! iterating over the unsorted ring and counting the number of slots that are less than versus
//! the number of slots that are greater than or equal to the claimed median. A genuine median
//! will necessarily have at most N/2 slots less than it, and at least (N/2)+1 slots less than or
//! equal to it.

#![no_std]

pub mod errors;
mod ring;
mod window;

use errors::InsufficientColdHeaders;
pub use ring::{Cold, Ring, RingValues, Warm};

mod window_access;
#[allow(unused_imports)]
pub(crate) use window_access::local::*;
pub use window_access::public::*;

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
