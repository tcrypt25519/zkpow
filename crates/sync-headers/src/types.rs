//! Core types for the header sync protocol.

use zkpow_core::{
    genesis_work, sha256d, target_from_bits, work_from_target, BlockHash, ChainWork, CompactTarget,
    Header as CoreHeader, Target, GENESIS_NBITS, GENESIS_TARGET, WINDOW_SIZE,
};

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
    /// Difficulty bits/target/work active at `height`, cached exactly like
    /// `zkpow_core::State::{current_nbits,current_target,current_work}` —
    /// updated only at a retarget boundary (`height % EPOCH_LENGTH == 0`),
    /// carried forward unchanged otherwise.
    pub current_nbits: CompactTarget,
    pub current_target: Target,
    pub current_work: ChainWork,
}

impl Tip {
    pub fn new(
        hash: BlockHash,
        height: u64,
        chainwork: ChainWork,
        timestamps: [u32; WINDOW_SIZE],
        current_nbits: CompactTarget,
        current_target: Target,
        current_work: ChainWork,
    ) -> Self {
        Self {
            hash,
            height,
            chainwork,
            timestamps,
            current_nbits,
            current_target,
            current_work,
        }
    }

    pub fn genesis(hash: BlockHash) -> Self {
        let mut timestamps = [0u32; WINDOW_SIZE];
        timestamps[0] = zkpow_core::MAINNET_GENESIS_HEADER.timestamp.into_inner();
        Self {
            hash,
            height: 0,
            chainwork: genesis_work(),
            timestamps,
            current_nbits: CompactTarget::new(GENESIS_NBITS),
            current_target: GENESIS_TARGET,
            current_work: genesis_work(),
        }
    }
}

/// Derive `(nbits, target, work)` for a tip whose latest header carries `bits`
/// — the one place outside a retarget boundary check that expands compact
/// bits, used to seed/refresh a `Tip`'s cached fields from a header's own bits.
pub fn current_difficulty(bits: CompactTarget) -> (CompactTarget, Target, ChainWork) {
    let target = target_from_bits(bits);
    let work = work_from_target(target).expect("target from valid compact bits is always valid");
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
        BlockHash::new(sha256d(&self.inner.to_bytes()))
    }

    pub fn prev_blockhash(&self) -> BlockHash {
        self.inner.prev_blockhash
    }
}

/// A validated header connected to the chain with its computed position and
/// cumulative work. Per-block work is computed in passing (to add into
/// `chainwork`) but isn't stored — nothing reads it once accumulated, and
/// `headers.db` has no column for it.
#[derive(Debug, Clone)]
pub struct ConnectedHeader {
    pub header: Header,
    pub height: u64,
    pub chainwork: ChainWork,
    pub median_time_past: u32,
}

impl ConnectedHeader {
    pub fn new(header: Header, height: u64, chainwork: ChainWork, median_time_past: u32) -> Self {
        Self {
            header,
            height,
            chainwork,
            median_time_past,
        }
    }

    pub fn block_hash(&self) -> BlockHash {
        self.header.block_hash()
    }

    pub fn prev_blockhash(&self) -> BlockHash {
        self.header.prev_blockhash()
    }
}
