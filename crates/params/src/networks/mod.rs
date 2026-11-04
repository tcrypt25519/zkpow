pub(crate) use super::*;

mod mainnet;
pub use mainnet::*;

#[cfg(any(test, feature = "test-networks"))]
mod short_epoch;
#[cfg(any(test, feature = "test-networks"))]
pub use short_epoch::*;
