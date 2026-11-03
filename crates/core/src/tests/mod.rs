use alloc::vec::Vec;

#[cfg(not(target_os = "zkvm"))]
extern crate std;

mod halt;
mod mtp;
mod pow;
mod public_values;
mod retarget;
mod state;

use zkpow_params::Mainnet as TestConsensus;

use super::*;

pub(crate) const GENESIS_NBITS: u32 = *TestConsensus::PARAMS.genesis.compact_target.as_inner();
pub(crate) const GENESIS_TARGET: Target = TestConsensus::PARAMS.genesis_target;

pub(crate) const EXPECTED_EPOCH_TIMESPAN: u32 = TestConsensus::PARAMS.expected_epoch_time();
pub(crate) const MIN_EPOCH_TIMESPAN: i64 = TestConsensus::PARAMS.min_epoch_timespan();
pub(crate) const MAX_EPOCH_TIMESPAN: i64 = TestConsensus::PARAMS.max_epoch_timespan();

pub(crate) fn expect_error_code(e: ApplyError) -> ErrorCode {
    e.error_code().expect("All ApplyErrors return an ErrorCode")
}

#[must_use]
pub(crate) fn set_hash_success() -> crate::test_hash_mock::MockHashGuard {
    crate::test_hash_mock::set_success()
}

#[must_use]
pub(crate) fn set_hash_fail() -> crate::test_hash_mock::MockHashGuard {
    crate::test_hash_mock::set_fail()
}

pub(crate) fn ts(seconds: u32) -> BlockTimestamp {
    BlockTimestamp::new(seconds)
}

pub(crate) fn zero_hash() -> BlockHash {
    BlockHash::new([0; 32])
}

pub(crate) fn sample_header() -> Header {
    Header {
        version:        1,
        prev_blockhash: BlockHash::new([0x11; 32]),
        merkle_root:    MerkleRoot::new([0x22; 32]),
        timestamp:      ts(10),
        compact_target: CompactTarget::new(GENESIS_NBITS),
        nonce:          7,
    }
}

pub(crate) fn hash_sample_header() -> BlockHash {
    hash_header(&sample_header())
}

pub(crate) fn test_state() -> State {
    State {
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        ..State::default()
    }
}

pub(crate) fn headers_from_timestamps(timestamps: &[u32]) -> Vec<HeaderInput> {
    timestamps
        .iter()
        .map(|timestamp| HeaderInput {
            version:     1,
            merkle_root: MerkleRoot::new([0x22; 32]),
            timestamp:   ts(*timestamp),
            nonce:       7,
        })
        .collect()
}

/// Each fixture record is an expected hash followed by a production Bitcoin header.
pub(crate) fn parse_fixture_blocks(fixture_bytes: &[u8]) -> Vec<(BlockHash, Header)> {
    let record_size = BlockHash::SIZE + Header::SIZE;
    let records = fixture_bytes.chunks_exact(record_size);
    assert!(records.remainder().is_empty(), "truncated header fixture");
    records
        .map(|record| {
            let (hash, header) = record.split_at(BlockHash::SIZE);
            (*BlockHash::parse(hash).unwrap(), *Header::parse(header).unwrap())
        })
        .collect()
}
