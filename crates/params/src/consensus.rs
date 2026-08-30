use sha256_const::double;
use zkpow_types::{BlockHash, BlockWork, CompactTarget, Header, Target};

/// The parameters that define a network.
///
/// Only independent values are fields. Anything derivable is a `const fn`, so there is exactly one
/// definition of each and a `C: Consensus` bound still folds down to literals at the call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsensusParams {
    pub genesis:        Header,
    /// The PoW limit, which is also the target the genesis header commits to.
    pub genesis_target: Target,
    /// Work proven by one block at [`ConsensusParams::genesis_target`].
    pub genesis_work:   BlockWork,

    /// Expected seconds between blocks.
    pub block_time:          u32,
    /// Blocks per difficulty epoch.
    pub epoch_length:        u32,
    /// A measured epoch may run this many times longer than expected before it is clamped.
    pub max_epoch_over_mul:  u8,
    /// ...and this many times shorter.
    pub min_epoch_under_div: u8,
    /// Number of previous block timestamps kept for median-time-past and ordered-window checks.
    pub window_size:         usize,
}

impl ConsensusParams {
    /// Half the window, rounded down; the number of timestamps that may sit below the median.
    #[must_use]
    pub const fn window_small_half_size(&self) -> usize {
        self.window_size / 2
    }

    /// Half the window, rounded up; the number of timestamps that may sit above the median.
    #[must_use]
    pub const fn window_large_half_size(&self) -> usize {
        (self.window_size + 1) / 2
    }

    /// Maximum index in the sliding window.
    #[must_use]
    pub const fn window_max_index(&self) -> usize {
        self.window_size - 1
    }

    /// SHA-256d of the genesis header. Prefer [`Consensus::GENESIS_HASH`], which holds the result
    /// of evaluating this once at compile time.
    #[must_use]
    pub const fn genesis_hash(&self) -> BlockHash {
        let bytes = self.genesis.to_bytes_const();
        BlockHash::new(double(&bytes))
    }

    /// Seconds a difficulty epoch is expected to take.
    #[must_use]
    pub const fn expected_epoch_time(&self) -> u32 {
        self.block_time * self.epoch_length
    }

    /// Upper clamp on a measured epoch timespan. `i64` because the measurement is a difference of
    /// two timestamps and can go negative.
    #[must_use]
    pub const fn max_epoch_timespan(&self) -> i64 {
        (self.expected_epoch_time() * self.max_epoch_over_mul as u32) as i64
    }

    /// Lower clamp on a measured epoch timespan.
    #[must_use]
    pub const fn min_epoch_timespan(&self) -> i64 {
        (self.expected_epoch_time() / self.min_epoch_under_div as u32) as i64
    }

    pub const fn compact_target(&self) -> CompactTarget {
        self.genesis.compact_target
    }

    pub const fn target(&self) -> Target {
        self.genesis_target
    }

    pub const fn block_work(&self) -> BlockWork {
        self.genesis_work
    }
}

/// Binds one parameter set to a type, so consensus code can be generic over the network and still
/// read every parameter as a compile-time constant.
pub trait Consensus {
    const PARAMS: ConsensusParams;

    /// Evaluated once per network, at compile time.
    const GENESIS_HASH: BlockHash = Self::PARAMS.genesis_hash();
}
