//! Core types for the header sync protocol.

use zkpow_core::{
    block_work_from_target, target_from_bits, BlockHash, BlockWork, ChainWork, CompactTarget, Header as CoreHeader,
    Target, GENESIS_NBITS, GENESIS_TARGET, GENESIS_WORK, WINDOW_SIZE,
};
use zkpow_sha256;

/// Network selection for Bitcoin consensus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Bitcoin,
    Testnet,
    Signet,
    Regtest,
}

/// The tip of the best known chain.
#[derive(Debug, Clone)]
pub struct Tip {
    pub hash: BlockHash,
    pub height: u64,
    pub chainwork: ChainWork,
    /// Ring buffer of the last (up to `WINDOW_SIZE`) block timestamps, slot
    /// `height % WINDOW_SIZE` — mirrors `zkpow_core::State::timestamps`.
    pub timestamps: [u32; WINDOW_SIZE],
    /// Count of consecutive nondecreasing timestamp links ending at this tip.
    /// Genesis has no predecessor, so its run is 0.
    pub ordered_timestamp_run: u64,

    /// Difficulty bits/target/block work active at `height`, cached by the sync path.
    /// Updated only at a retarget boundary (`height % EPOCH_LENGTH == 0`) and
    /// left unchanged otherwise.
    /// TODO: Just put this in a separate table of epochs. Reader selects from epochs where id = height / EPOCH_LENGTH
    pub current_nbits:      CompactTarget,
    pub current_target:     Target,
    pub current_block_work: BlockWork,
}

impl Tip {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        hash: BlockHash,
        height: u64,
        chainwork: ChainWork,
        timestamps: [u32; WINDOW_SIZE],
        ordered_timestamp_run: u64,
        current_nbits: CompactTarget,
        current_target: Target,
        current_block_work: BlockWork,
    ) -> Self {
        Self {
            hash,
            height,
            chainwork,
            timestamps,
            ordered_timestamp_run,
            current_nbits,
            current_target,
            current_block_work,
        }
    }

    pub fn genesis(hash: BlockHash) -> Self {
        let mut timestamps = [0u32; WINDOW_SIZE];
        timestamps[0] = zkpow_core::MAINNET_GENESIS_HEADER.timestamp.into_inner();
        let genesis_work = GENESIS_WORK;
        let genesis_block_work = block_work_from_target(GENESIS_TARGET);
        Self {
            hash,
            height: 0,
            chainwork: genesis_work,
            timestamps,
            ordered_timestamp_run: 0,
            current_nbits: CompactTarget::new(GENESIS_NBITS),
            current_target: GENESIS_TARGET,
            current_block_work: genesis_block_work,
        }
    }
}

/// Expand `bits` into the `(nbits, target, block_work)` triple a `Tip` caches.
pub fn expand_target(bits: CompactTarget) -> (CompactTarget, Target, BlockWork) {
    let target = target_from_bits(bits);
    let work = block_work_from_target(target);
    (bits, target, work)
}

/// A raw block header received from the network.
#[derive(Debug, Clone)]
pub struct Header {
    pub inner: CoreHeader,
}

impl Header {
    pub fn new(inner: CoreHeader) -> Self {
        Self { inner }
    }

    pub fn block_hash(&self) -> BlockHash {
        BlockHash::new(zkpow_sha256::double(&self.inner.to_bytes()))
    }

    pub fn prev_blockhash(&self) -> BlockHash {
        self.inner.prev_blockhash
    }
}

#[derive(Debug, Clone)]
pub struct ConnectedHeader {
    pub header: Header,
    pub block_hash: BlockHash,
    pub height: u64,
    pub chainwork: ChainWork,
    pub median_time_past: u32,
    pub ordered_timestamp_run: u64,
}

impl ConnectedHeader {
    pub fn new(
        header: Header,
        block_hash: BlockHash,
        height: u64,
        chainwork: ChainWork,
        median_time_past: u32,
        ordered_timestamp_run: u64,
    ) -> Self {
        Self {
            header,
            block_hash,
            height,
            chainwork,
            median_time_past,
            ordered_timestamp_run,
        }
    }

    pub fn block_hash(&self) -> BlockHash {
        self.block_hash
    }

    pub fn prev_blockhash(&self) -> BlockHash {
        self.header.prev_blockhash()
    }
}
