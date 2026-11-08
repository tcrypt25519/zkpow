//! Core types for the header sync protocol.

use strum_macros::{Display, EnumCount, EnumString};
use zkpow_core::{block_work_from_target, target_from_bits};
use zkpow_mtp::{Cold, Window, WindowState};
use zkpow_params::Consensus;
use zkpow_types::{BlockHash, ChainWork, CompactTarget, Header as CoreHeader, TargetTriplet};

/// Network selection for Bitcoin consensus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, EnumCount)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
#[repr(u8)]
pub enum Network {
    Mainnet,
    Testnet,
    Regtest,
}

impl Network {
    pub fn parse(s: &str) -> Option<Self> {
        str::parse(s).ok()
    }
}

/// The tip of the best known chain.
#[derive(Debug, Clone)]
pub struct Tip {
    pub hash:            BlockHash,
    pub height:          u32,
    pub chainwork:       ChainWork,
    pub timestamps:      WindowState,
    pub epoch_timestamp: u32,

    /// Difficulty bits/target/block work active at `height`, cached by the sync path.
    /// Updated only at a retarget boundary (`height % EPOCH_LENGTH == 0`) and
    /// left unchanged otherwise.
    /// TODO: Just put this in a separate table of epochs. Reader selects from epochs where id = height / EPOCH_LENGTH
    pub target_triplet: TargetTriplet,
}

impl Tip {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        hash: BlockHash,
        height: u32,
        chainwork: ChainWork,
        timestamps: WindowState,
        epoch_timestamp: u32,
        target_triplet: TargetTriplet,
    ) -> Self {
        Self {
            hash,
            height,
            chainwork,
            timestamps,
            epoch_timestamp,
            target_triplet,
        }
    }

    pub fn genesis<C: Consensus>(hash: BlockHash) -> Self {
        let genesis = C::PARAMS.genesis;
        let epoch_timestamp = genesis.timestamp.into_inner();
        let timestamps = WindowState::Cold(Window::<Cold>::new(genesis.timestamp));
        let target_triplet = expand_target(genesis.compact_target);
        Self {
            hash,
            height: 0,
            chainwork: ChainWork::new(target_triplet.block_work().into_inner()),
            timestamps,
            epoch_timestamp,
            target_triplet,
        }
    }
}

/// Expand `bits` into the `(nbits, target, block_work)` triple a `Tip` caches.
pub fn expand_target(bits: CompactTarget) -> TargetTriplet {
    let target = target_from_bits(bits);
    let work = block_work_from_target(&target);
    TargetTriplet::new(bits, target, work)
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
        BlockHash::from(&self.inner)
    }

    pub fn prev_blockhash(&self) -> BlockHash {
        self.inner.prev_blockhash
    }
}

/// A header linked onto the chain, with the chain values derived at its height.
#[derive(Debug, Clone)]
pub struct ConnectedHeader {
    pub header:           Header,
    pub block_hash:       BlockHash,
    pub chainwork:        ChainWork,
    pub height:           u32,
    pub median_time_past: u32,
}

impl ConnectedHeader {
    pub fn new(
        header: Header,
        block_hash: BlockHash,
        height: u32,
        chainwork: ChainWork,
        median_time_past: u32,
    ) -> Self {
        Self {
            header,
            block_hash,
            height,
            chainwork,
            median_time_past,
        }
    }

    pub fn block_hash(&self) -> BlockHash {
        self.block_hash
    }

    pub fn prev_blockhash(&self) -> BlockHash {
        self.header.prev_blockhash()
    }
}
