//! Median time past over a ring of block timestamps.
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

#![no_std]

pub mod errors;
mod ring;

use errors::InsufficientColdHeaders;
pub use ring::{Cold, Ring, RingValues, Warm, Window, WindowState, WindowValues};

/// Number of previous block timestamps kept for median-time-past and ordered-window checks.
pub const WINDOW_SIZE: usize = 11;

#[cfg(test)]
mod tests;
mod window;

/// Splits headers into the prefix that fills an empty consensus window and the warm remainder.
pub fn split_off_cold_headers<T>(headers: &[T]) -> Result<(&[T], &[T]), InsufficientColdHeaders> {
    let required = WINDOW_SIZE - 1;
    if headers.len() < required {
        return Err(InsufficientColdHeaders::new(headers.len()));
    }
    Ok(headers.split_at(required))
}
