//! A reproducible fake header chain for tests.
//!
//! Everything a miner would choose freely (merkle root, nonce, how long a block took) is drawn from
//! a PRNG seeded once, so a seed plus a starting point names one exact chain. Everything consensus
//! fixes is not invented: each header links to the previous one's hash, and the chain retargets on
//! the same schedule and from the same two timestamps [`State`](zkpow_core::state::State) does. A run generated
//! from height zero therefore applies cleanly to a state seeded from genesis.
//!
//! No nonce search happens, so a header's hash meets the target it carries only by luck. Tests that
//! exercise the proof-of-work check need either a starting difficulty loose enough that luck is
//! enough or a mocked hash.

use core::marker::PhantomData;

use zkpow_core::{
    BlockHash, BlockTimestamp, CompactTarget, Header, calculate_next_target_required, target_from_bits,
    timestamp_count, timestamp_slot,
};
use zkpow_params::{Consensus, WINDOW_SIZE};

/// An endless chain of fake headers for `C`, starting from a tip the seed and the builder methods
/// determine.
///
/// [`Chain::new`] starts at the network's genesis block, so the first header yielded is height
/// one. [`Chain::at_height`] and [`Chain::with_nbits`] move the starting point; both replay
/// the chain from the seed, so their order does not matter.
pub struct Chain<C: Consensus> {
    seed:            u64,
    prng:            SplitMix64,
    height:          u32,
    tip_hash:        BlockHash,
    nbits:           CompactTarget,
    epoch_timestamp: BlockTimestamp,
    timestamps:      [BlockTimestamp; WINDOW_SIZE],
    exhausted:       bool,
    network:         PhantomData<fn() -> C>,
}

impl<C: Consensus> Chain<C> {
    /// A chain seeded from `seed`, sitting on the network's genesis block at its genesis difficulty.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self::start(seed, 0, C::PARAMS.compact_target())
    }

    /// Move the starting tip to `height`, keeping the seed and the starting difficulty.
    ///
    /// Above zero the tip is synthetic: its hash comes from the seed and its timestamp window is
    /// laid out at exactly one `block_time` per block, which also fixes where its epoch began.
    #[must_use]
    pub fn at_height(self, height: u32) -> Self {
        Self::start(self.seed, height, self.nbits)
    }

    /// Start at `nbits` rather than the network's genesis difficulty. The chain retargets away from
    /// it at the next epoch boundary like any other value.
    #[must_use]
    pub fn with_nbits(self, nbits: CompactTarget) -> Self {
        Self::start(self.seed, self.height, nbits)
    }

    /// The height of the tip. The next header yielded is one above it.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The hash of the tip, which the next header extends.
    #[must_use]
    pub fn tip_hash(&self) -> BlockHash {
        self.tip_hash
    }

    /// The difficulty the next header carries, unless it opens a new epoch.
    #[must_use]
    pub fn nbits(&self) -> CompactTarget {
        self.nbits
    }

    /// The timestamp of the tip.
    #[must_use]
    pub fn tip_timestamp(&self) -> BlockTimestamp {
        self.timestamps[timestamp_slot(self.height)]
    }

    /// The timestamp the current epoch's retarget measures from.
    #[must_use]
    pub fn epoch_timestamp(&self) -> BlockTimestamp {
        self.epoch_timestamp
    }

    /// The sliding timestamp window as of the tip, in [`State`](zkpow_core::state::State)'s slot order.
    #[must_use]
    pub fn timestamps(&self) -> [BlockTimestamp; WINDOW_SIZE] {
        self.timestamps
    }

    fn start(seed: u64, height: u32, nbits: CompactTarget) -> Self {
        let mut prng = SplitMix64::new(seed);
        let block_time = C::PARAMS.block_time;
        let genesis_timestamp = C::PARAMS.genesis.timestamp.into_inner();

        let (tip_hash, tip_timestamp) = if height == 0 {
            (C::GENESIS_HASH, genesis_timestamp)
        } else {
            let elapsed = height.saturating_mul(block_time);
            (BlockHash::new(prng.next_bytes()), genesis_timestamp.saturating_add(elapsed))
        };

        let mut timestamps = [BlockTimestamp::default(); WINDOW_SIZE];
        for offset in 0..timestamp_count(height) as u32 {
            let slot = timestamp_slot(height - offset);
            timestamps[slot] = BlockTimestamp::new(tip_timestamp.saturating_sub(offset.saturating_mul(block_time)));
        }

        let into_epoch = (height % C::PARAMS.epoch_length).saturating_mul(block_time);

        Self {
            seed,
            prng,
            height,
            tip_hash,
            nbits,
            epoch_timestamp: BlockTimestamp::new(tip_timestamp.saturating_sub(into_epoch)),
            timestamps,
            exhausted: false,
            network: PhantomData,
        }
    }

    /// Seconds between the tip and the next block: uniform over `[block_time / 2, block_time * 3/2]`,
    /// so the mean interval is the expected one and every retarget measures a span that moved.
    fn interval(&mut self) -> u32 {
        let block_time = C::PARAMS.block_time;
        let jitter = self.prng.next_u32() % block_time.saturating_add(1);
        (block_time / 2).saturating_add(jitter).max(1)
    }
}

impl<C: Consensus> Iterator for Chain<C> {
    type Item = Header;

    fn next(&mut self) -> Option<Header> {
        if self.exhausted {
            return None;
        }

        let Some(height) = self.height.checked_add(1) else {
            self.exhausted = true;
            return None;
        };
        let interval = self.interval();
        let Some(timestamp) = self.tip_timestamp().into_inner().checked_add(interval) else {
            self.exhausted = true;
            return None;
        };

        let opens_epoch = height.is_multiple_of(C::PARAMS.epoch_length);
        if opens_epoch {
            let (nbits, ..) = calculate_next_target_required::<C>(
                target_from_bits(self.nbits),
                self.epoch_timestamp,
                self.tip_timestamp(),
            );
            self.nbits = nbits;
        }

        let header = Header {
            version:        1,
            prev_blockhash: self.tip_hash,
            merkle_root:    self.prng.next_bytes(),
            timestamp:      BlockTimestamp::new(timestamp),
            compact_target: self.nbits,
            nonce:          self.prng.next_u32(),
        };

        self.height = height;
        self.tip_hash = BlockHash::from(&header);
        self.timestamps[timestamp_slot(height)] = header.timestamp;
        if opens_epoch {
            self.epoch_timestamp = header.timestamp;
        }

        Some(header)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.exhausted {
            return (0, Some(0));
        }
        // An upper bound only: a chain can also end early by running its timestamps past `u32::MAX`.
        (0, Some((u32::MAX - self.height) as usize))
    }
}

impl<C: Consensus> core::iter::FusedIterator for Chain<C> {}

/// SplitMix64. Small enough to carry rather than depend on, and its sequence is fixed by the
/// algorithm, so a seed keeps naming the same chain no matter what the crate's dependencies do.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    fn next_bytes(&mut self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        for chunk in bytes.chunks_mut(8) {
            chunk.copy_from_slice(&self.next_u64().to_le_bytes());
        }
        bytes
    }
}
