use super::*;
use crate::{Consensus, ConsensusParams};

/// Bitcoin mainnet.
pub struct Mainnet;

impl Consensus for Mainnet {
    const PARAMS: ConsensusParams = MAINNET_PARAMS;
}

pub(crate) const MAINNET_PARAMS: ConsensusParams = ConsensusParams {
    genesis:        Header {
        version:        1,
        prev_blockhash: BlockHash::new([0u8; 32]),
        // The genesis coinbase merkle root, in internal (reversed-from-display) byte order.
        merkle_root:    MerkleRoot::new([
            0x3b, 0xa3, 0xed, 0xfd, 0x7a, 0x7b, 0x12, 0xb2, 0x7a, 0xc7, 0x2c, 0x3e, 0x67, 0x76, 0x8f, 0x61, 0x7f, 0xc8,
            0x1b, 0xc3, 0x88, 0x8a, 0x51, 0x32, 0x3a, 0x9f, 0xb8, 0xaa, 0x4b, 0x1e, 0x5e, 0x4a,
        ]),
        timestamp:      BlockTimestamp::new(1231006505),
        compact_target: CompactTarget::new(0x1d00ffff),
        nonce:          2083236893,
    },
    // The PoW limit: 0x1d00ffff expanded, i.e. 0xFFFF << 208.
    genesis_target: Target::from_limbs([0, 0, 0, 0x0000_0000_FFFF_0000]),

    // Work produced by one block at the genesis target. At height 0 this is
    // also the chain's `ChainWork`.
    genesis_work:        BlockWork::from_limbs([0x0000_0001_0001_0001, 0, 0, 0]),
    block_time:          600,
    epoch_length:        2016,
    max_epoch_over_mul:  4,
    min_epoch_under_div: 4,
};
