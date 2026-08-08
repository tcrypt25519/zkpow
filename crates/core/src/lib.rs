//! Shared consensus types and pure helper logic for the zkpow prover.

#![no_std]

#[cfg(test)]
mod tests;

extern crate alloc;
extern crate self as zkpow_core;

use bytemuck::Pod;

mod header;
pub use header::{Header, HeaderInput};

pub mod primitives;
pub use primitives::{BlockHash, BlockTimestamp, BlockWork, ByteInfo, ChainWork, CompactTarget, Target, u256};

mod public_values;
pub use public_values::{
    ApplyFailure, Claim, ContinuationData, ErrorCode, ErrorCodeNames, MinimalPublicValues, PublicValuesDigest,
    ReturnCode, VerifierKeyDigest, serialize_minimal_public_values,
};
pub use sp1_cycle;

mod state;
pub use state::State;

mod errors;
pub use errors::{InputParseError, ParseError};

mod input;
pub use input::{cast_slice, serialize_inputs};
#[cfg(not(target_os = "zkvm"))]
use sha2::{Digest, Sha256};

use crate::primitives::NonMaxU256;

/// Sliding window size used for median-time-past checks.
pub const WINDOW_SIZE: usize = 11;

/// Half of the sliding window size.
pub const WINDOW_SMALL_HALF_SIZE: usize = WINDOW_SIZE / 2;

// Half of the sliding window size, rounded up. Used wind around the buffer
pub const WINDOW_LARGE_HALF_SIZE: usize = WINDOW_SIZE - WINDOW_SMALL_HALF_SIZE;

/// Maximum index in the sliding window.
pub const WINDOW_MAX_INDEX: usize = WINDOW_SIZE - 1;

/// Number of blocks per difficulty retarget epoch.
pub const EPOCH_LENGTH: u32 = 2016;

/// Expected seconds between consecutive blocks.
pub const TARGET_BLOCK_TIME: u32 = 600;

/// Expected timespan of a single difficulty epoch in seconds.
pub const EXPECTED_EPOCH_TIMESPAN: u32 = EPOCH_LENGTH * TARGET_BLOCK_TIME;

/// Minimum clamped epoch timespan used for difficulty retargeting.
pub const MIN_EPOCH_TIMESPAN: i64 = (EXPECTED_EPOCH_TIMESPAN / 4) as i64;

/// Maximum clamped epoch timespan used for difficulty retargeting.
pub const MAX_EPOCH_TIMESPAN: i64 = (EXPECTED_EPOCH_TIMESPAN * 4) as i64;

/// Mainnet PoW limit in compact form.
pub const GENESIS_NBITS: u32 = 0x1d00ffff;

/// Mainnet PoW limit as an expanded 256-bit target (the full expansion of `0x1d00ffff`).
///
/// Stored as four little-endian u64 limbs. The value is:
///   0x00000000FFFF0000000000000000000000000000000000000000000000000000
/// In LE limbs: limbs[3] = 0xFFFF0000, limbs[0..2] = 0.
pub const GENESIS_TARGET: Target = Target::new(NonMaxU256::from_limbs([0, 0, 0, 0x0000_0000_FFFF_0000]));

pub const GENESIS_WORK: ChainWork = ChainWork::from_limbs([0x0000_0001_0001_0001, 0, 0, 0]);

/// Size of a serialized [`Claim`] in bytes.
pub const CLAIM_SIZE: usize = 32 + 32 + 32 + 4;

/// Size of the serialized [`ContinuationData`] in bytes (no padding).
pub const CONTINUATION_DATA_SIZE: usize = 4 + 4 + 4 * WINDOW_SIZE;

/// Size of the serialized [`PublicValues`] in bytes.
pub const MINIMAL_PV_SIZE: usize = 32 + 32 + 32 + 32 + 32 + 4 + 1;

/// Raw little-endian mainnet genesis block hash.
pub const MAINNET_GENESIS_HASH: BlockHash = BlockHash::new([
    0x6f, 0xe2, 0x8c, 0x0a, 0xb6, 0xf1, 0xb3, 0x72, 0xc1, 0xa6, 0xa2, 0x46, 0xae, 0x63, 0xf7, 0x4f, 0x93, 0x1e, 0x83,
    0x65, 0xe1, 0x5a, 0x08, 0x9c, 0x68, 0xd6, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00,
]);

/// The mainnet genesis block header (height 0). `prev_blockhash` is the zero
/// hash; `merkle_root` is the genesis coinbase merkle root in internal
/// (reversed-from-display) byte order.
pub const MAINNET_GENESIS_HEADER: Header = Header {
    version:        1,
    prev_blockhash: BlockHash::new([0u8; 32]),
    merkle_root:    [
        0x3b, 0xa3, 0xed, 0xfd, 0x7a, 0x7b, 0x12, 0xb2, 0x7a, 0xc7, 0x2c, 0x3e, 0x67, 0x76, 0x8f, 0x61, 0x7f, 0xc8,
        0x1b, 0xc3, 0x88, 0x8a, 0x51, 0x32, 0x3a, 0x9f, 0xb8, 0xaa, 0x4b, 0x1e, 0x5e, 0x4a,
    ],
    timestamp:      BlockTimestamp::new(1231006505),
    compact_target: CompactTarget::new(GENESIS_NBITS),
    nonce:          2083236893,
};

#[cfg(target_os = "zkvm")]
fn hash_header(
    version: u32,
    prev_blockhash: &BlockHash,
    merkle_root: [u8; 32],
    timestamp: BlockTimestamp,
    nbits: CompactTarget,
    nonce: u32,
) -> BlockHash {
    let hash = zkpow_sha256::of_header(
        version,
        prev_blockhash.into_inner(),
        merkle_root,
        timestamp.into_inner(),
        nbits.into_inner(),
        nonce,
    );
    BlockHash::new(hash)
}

#[cfg(not(target_os = "zkvm"))]
fn host_hash_header(
    version: u32,
    prev_blockhash: &BlockHash,
    merkle_root: [u8; 32],
    timestamp: BlockTimestamp,
    nbits: CompactTarget,
    nonce: u32,
) -> BlockHash {
    let mut hasher = Sha256::new();
    hasher.update(version.to_le_bytes());
    hasher.update(prev_blockhash.as_inner());
    hasher.update(merkle_root);
    hasher.update(timestamp.to_le_bytes());
    hasher.update(nbits.to_le_bytes());
    hasher.update(nonce.to_le_bytes());
    let hash = hasher.finalize();
    BlockHash::new(*hash.as_array().expect("sha256 output is 32 bytes"))
}

#[cfg(not(target_os = "zkvm"))]
fn hash_header(
    version: u32,
    prev_blockhash: &BlockHash,
    merkle_root: [u8; 32],
    timestamp: BlockTimestamp,
    nbits: CompactTarget,
    nonce: u32,
) -> BlockHash {
    #[cfg(test)]
    if let Some(hash) = test_hash_mock::get() {
        return BlockHash::new(hash);
    }
    host_hash_header(version, prev_blockhash, merkle_root, timestamp, nbits, nonce)
}

#[cfg(test)]
mod test_hash_mock {
    extern crate std;
    use std::cell::Cell;

    std::thread_local! {
        static MOCK_HEADER_HASH: Cell<Option<[u8; 32]>> = const { Cell::new(None) };
    }

    pub(crate) struct MockHashGuard;

    pub(crate) fn get() -> Option<[u8; 32]> {
        MOCK_HEADER_HASH.with(|mock| mock.get())
    }

    pub(crate) fn set_success() -> MockHashGuard {
        set([0; 32])
    }

    pub(crate) fn set_fail() -> MockHashGuard {
        set([0xFF; 32])
    }

    fn set(hash: [u8; 32]) -> MockHashGuard {
        MOCK_HEADER_HASH.with(|mock| {
            assert!(mock.get().is_none(), "hash mock already set in this thread");
            mock.set(Some(hash));
        });
        MockHashGuard
    }

    impl Drop for MockHashGuard {
        fn drop(&mut self) {
            MOCK_HEADER_HASH.with(|mock| mock.set(None));
        }
    }
}

#[must_use]
pub const fn timestamp_slot(height: u64) -> usize {
    height as usize % WINDOW_SIZE
}

#[must_use]
pub const fn timestamp_count(height: u64) -> usize {
    let count = height as usize + 1;
    if count > WINDOW_SIZE { WINDOW_SIZE } else { count }
}

#[must_use]
pub const fn has_ordered_timestamp_window(ordered_timestamp_run: u64) -> bool {
    ordered_timestamp_run >= (WINDOW_MAX_INDEX) as u64
}

#[must_use]
pub const fn advance_ordered_timestamp_run(ordered_timestamp_run: u64, previous_timestamp: u32, timestamp: u32) -> u64 {
    if timestamp >= previous_timestamp {
        ordered_timestamp_run.saturating_add(1)
    } else {
        0
    }
}

#[must_use]
pub fn ordered_timestamp_run(timestamps: &[BlockTimestamp; WINDOW_SIZE], height: u64) -> u64 {
    let mut run = 0;
    let mut newer = timestamps[timestamp_slot(height)];
    for offset in 1..WINDOW_SIZE {
        let older = timestamps[(timestamp_slot(height) + WINDOW_SIZE - offset) % WINDOW_SIZE];
        if older > newer {
            break;
        }
        run += 1;
        newer = older;
    }
    run
}

#[must_use]
pub const fn ordered_median_timestamp_slot(height: u64) -> usize {
    (timestamp_slot(height) + WINDOW_SIZE - (WINDOW_SMALL_HALF_SIZE)) % WINDOW_SIZE
}

#[must_use]
pub const fn requires_median_hint(height: u64, previous_ordered_timestamp_run: u64) -> bool {
    height >= WINDOW_SIZE as u64 && !has_ordered_timestamp_window(previous_ordered_timestamp_run)
}

#[sp1_cycle::tracked]
pub(crate) fn check_exact_len(bytes: &[u8], expected: usize) -> Result<(), ParseError> {
    if bytes.len() != expected {
        return Err(ParseError::InvalidLength {
            expected,
            actual: bytes.len(),
        });
    }
    Ok(())
}

#[sp1_cycle::tracked]
pub(crate) fn copy_to_bytes<const N: usize, T: Pod>(value: &T) -> [u8; N] {
    *bytemuck::bytes_of::<T>(value).as_array().expect("a")
}

/// Convert a full 256-bit target into compact `bits` encoding.
#[must_use]
//#[sp1_cycle::tracked]
pub fn target_to_bits(target: Target) -> CompactTarget {
    let bytes = target.to_le_bytes();
    let mut nbytes = 32usize;
    while nbytes > 0 && bytes[nbytes - 1] == 0 {
        nbytes -= 1;
    }
    if nbytes == 0 {
        return CompactTarget::new(0);
    }

    let mut compact = if nbytes <= 3 {
        let mut value = 0u32;
        for i in (0..nbytes).rev() {
            value = (value << 8) | bytes[i] as u32;
        }
        value << (8 * (3 - nbytes))
    } else {
        ((bytes[nbytes - 1] as u32) << 16) | ((bytes[nbytes - 2] as u32) << 8) | (bytes[nbytes - 3] as u32)
    };

    if (compact & 0x0080_0000) != 0 {
        compact >>= 8;
        nbytes += 1;
    }

    CompactTarget::new(((nbytes as u32) << 24) | (compact & 0x007f_ffff))
}

/// Expand compact Bitcoin `bits` into a normalized full 256-bit target.
#[must_use]
#[sp1_cycle::tracked]
pub fn target_from_bits(compact_target: CompactTarget) -> Target {
    let bits = compact_target.into_inner();
    let size = (bits >> 24) as usize;
    let mantissa = bits & 0x007f_ffff;
    let mut normalized_target_bytes = [0u8; 32];

    if size <= 3 {
        let value = mantissa >> (8 * (3 - size));
        normalized_target_bytes[0..4].copy_from_slice(&value.to_le_bytes());
    } else {
        let offset = size - 3;
        if offset < 32 {
            normalized_target_bytes[offset] = (mantissa & 0xff) as u8;
        }
        if offset + 1 < 32 {
            normalized_target_bytes[offset + 1] = ((mantissa >> 8) & 0xff) as u8;
        }
        if offset + 2 < 32 {
            normalized_target_bytes[offset + 2] = ((mantissa >> 16) & 0xff) as u8;
        }
    }

    let target = NonMaxU256::new(u256::from_le_bytes(normalized_target_bytes))
        .expect("a compact target cannot expand to u256::MAX");
    Target::new(target)
}

/// Check whether a header hash satisfies a target.
#[must_use]
#[sp1_cycle::tracked]
pub fn check_proof_of_work(hash: BlockHash, target: &Target) -> bool {
    u256::from_le_bytes(*hash.as_inner()) <= *target.as_inner().as_inner()
}

/// Subtract `rhs` from `self` in place. Returns `true` if there was an underflow (borrow out).
#[must_use]
fn sub_assign(lhs: &mut u256, rhs: u256) -> bool {
    let mut borrow = false;

    let lhs_limbs: &mut [u64; 4] = lhs.as_limbs_mut();
    let rhs_limbs = rhs.as_limbs();

    let (a, b1) = lhs_limbs[0].overflowing_sub(rhs_limbs[0]);
    let (a, b2) = a.overflowing_sub(borrow as u64);
    lhs_limbs[0] = a;
    borrow = b1 || b2;

    let (a, b1) = lhs_limbs[1].overflowing_sub(rhs_limbs[1]);
    let (a, b2) = a.overflowing_sub(borrow as u64);
    lhs_limbs[1] = a;
    borrow = b1 || b2;

    let (a, b1) = lhs_limbs[2].overflowing_sub(rhs_limbs[2]);
    let (a, b2) = a.overflowing_sub(borrow as u64);
    lhs_limbs[2] = a;
    borrow = b1 || b2;

    let (a, b1) = lhs_limbs[3].overflowing_sub(rhs_limbs[3]);
    let (a, b2) = a.overflowing_sub(borrow as u64);
    lhs_limbs[3] = a;
    borrow = b1 || b2;

    borrow
}

/// Add one to a target, modifying the limbs in place.
///
/// A valid target can not be a max u256, so:
///   - It can't carry over into a 5th limb
///   - A carry can only start if the first limb is max u64
///   - A carry can only continue if the next limb is a max u64
///   - A max u64 with a carry-over of 1 becomes 0
///
/// We can iterate until we find a limb < max u64, add 1 to it, and we're done. As long as we find max u64 limbs we
/// just set them to 0 and move forward. [`NonMaxU256`] guarantees that the carry cannot escape the final limb.
pub(crate) fn increment_target(target: NonMaxU256) -> u256 {
    let mut target = target.into_inner();
    for (i, limb) in target.as_limbs_mut().iter_mut().enumerate() {
        // This limb isn't saturated so it can't carry; add 1 and we're done.
        if *limb < u64::MAX {
            *limb += 1;
            break;
        }

        // This is unreachable because NonMaxU256 excludes the only value that could carry out.
        if i == 3 {
            unreachable!("NonMaxU256 invariant violated");
        }

        // This limb is saturated but not the final limb, so roll it over to 0 and proceed with the carry
        *limb = 0;
    }
    target
}

/// Shift `value` left by one bit in place, returning the carry (the evicted MSB).
#[must_use]
fn shl1_with_carry(value: &mut u256) -> bool {
    let limbs = value.as_limbs_mut();
    let mut carry = 0u64;
    for limb in limbs.iter_mut() {
        let next_carry = *limb >> 63;
        *limb = (*limb << 1) | carry;
        carry = next_carry;
    }
    carry > 0
}

/// Compute one-block cumulative-work units from the expanded target.
///
/// Uses non-restoring binary long division to compute `floor(2^256 / (target + 1))`.
/// The loop processes 257 bits (indices 256 down to 0): at `bit == 256` we introduce
/// the implicit leading 1 of the dividend `2^256`; for `bit < 256` we set the
/// corresponding quotient bit when the remainder meets the divisor.
#[sp1_cycle::tracked]
pub fn block_work_from_target(target: Target) -> BlockWork {
    let divisor = increment_target(target.into_inner());

    let mut remainder = u256::from_limbs([0u64; 4]);
    let mut quotient = [0u64; 4];

    sp1_cycle::track!("zkpow_core::lib::work_from_target.loop", {
        for bit in (0..=256u32).rev() {
            // Shift remainder left, absorbing the next dividend bit.
            // At bit == 256 the dividend is 2^256, so its MSB is 1; all lower bits are 0.
            let carry = shl1_with_carry(&mut remainder);
            if bit == 256 {
                // Inject the implicit leading 1 of 2^256 into the LSB of the shifted remainder.
                remainder.as_limbs_mut()[0] |= 1;
            }
            // If carry is set the remainder is effectively > 2^256, which is always >= divisor.

            if carry || remainder >= divisor {
                let _ = sub_assign(&mut remainder, divisor);
                if bit < 256 {
                    quotient[(bit / 64) as usize] |= 1u64 << (bit % 64);
                }
            }
        }
    });

    BlockWork::from_limbs(quotient)
}

/// Compute the compact and normalized full target required at a difficulty boundary.
#[must_use]
pub fn calculate_next_target_required(
    old_target: Target,
    epoch_start_timestamp: BlockTimestamp,
    previous_timestamp: BlockTimestamp,
) -> (CompactTarget, Target, BlockWork) {
    // Normalize the timespan to ensure it's within the min/max bounds allowed.
    let actual_timespan = i64::from(*previous_timestamp) - i64::from(*epoch_start_timestamp);
    let clamped_timespan = actual_timespan.clamp(MIN_EPOCH_TIMESPAN, MAX_EPOCH_TIMESPAN) as u32;

    let mut calculated_target = old_target.into_inner().into_inner();
    calculated_target *= clamped_timespan;

    let mut remainder = 0u128;
    for limb in calculated_target.as_limbs_mut().iter_mut().rev() {
        let value = (remainder << 64) | (*limb as u128);
        *limb = (value / (EXPECTED_EPOCH_TIMESPAN as u128)) as u64;
        remainder = value % (EXPECTED_EPOCH_TIMESPAN as u128);
    }

    // Normalize the new target to ensure an upper-bound (a minimum difficulty) and to handle the implicit
    // truncation that occurs when round-tripping through the nbits format.
    let calculated_target =
        Target::new(NonMaxU256::new(calculated_target).expect("a scaled Bitcoin target cannot be u256::MAX"));
    let clamped_target: Target = calculated_target.min(GENESIS_TARGET);
    let compact_target = target_to_bits(clamped_target);
    let normalized_target = target_from_bits(compact_target);

    let block_work = block_work_from_target(normalized_target);

    // The target is capped above and truncated below; return it and the compact form we calculated along the way.
    (compact_target, normalized_target, block_work)
}
