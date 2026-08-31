//! The header chain state transition and the consensus math it runs on.
//!
//! The value types live in `zkpow-types` and the network parameters in `zkpow-params`; this crate
//! is generic over the latter. The types are re-exported here so consumers have a single import
//! surface for a header chain's vocabulary.

#![no_std]

#[cfg(test)]
mod tests;

extern crate alloc;
extern crate self as zkpow_core;

use zkpow_params::{Consensus, Mainnet};
pub use zkpow_types::{
    BlockHash, BlockTimestamp, BlockWork, ByteInfo, ChainWork, CompactTarget, Header, NonMaxU256, Target, u256,
};

mod header;
pub use header::HeaderInput;

mod public_values;
pub use public_values::{ErrorCode, PublicValues, ReturnCode, VerifierKeyDigest};
pub use sp1_cycle;

mod state;
pub use state::State;

#[cfg(any(test, feature = "test-support"))]
mod chain;
#[cfg(any(test, feature = "test-support"))]
pub use chain::TestChain;

mod errors;
pub use errors::{ApplyError, ParseError, ReportableError};

pub mod halt;
mod input;
pub use input::{cast_slice, serialize_inputs};

/// Hashing lives on [`Header`]; this wrapper exists only so tests can substitute a hash without
/// mining one. It must stay in this crate: `cfg(test)` is per-compilation-unit, so a mock in
/// `zkpow-types` would not be compiled when `zkpow-core`'s tests run.
#[sp1_cycle::tracked]
fn hash_header(header: &Header) -> BlockHash {
    #[cfg(test)]
    if let Some(hash) = test_hash_mock::get() {
        return BlockHash::new(hash);
    }
    BlockHash::from(header)
}

#[sp1_cycle::tracked]
fn hash_header_input(input: &HeaderInput, prev_blockhash: &BlockHash, compact_target: CompactTarget) -> BlockHash {
    #[cfg(target_os = "zkvm")]
    {
        BlockHash::new(zkpow_sha256::of_header(
            input.version,
            *prev_blockhash.as_inner(),
            input.merkle_root,
            *input.timestamp.as_inner(),
            compact_target.into_inner(),
            input.nonce,
        ))
    }

    #[cfg(not(target_os = "zkvm"))]
    {
        let header = Header {
            version: input.version,
            prev_blockhash: *prev_blockhash,
            merkle_root: input.merkle_root,
            timestamp: input.timestamp,
            compact_target,
            nonce: input.nonce,
        };
        hash_header(&header)
    }
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
        MOCK_HEADER_HASH.get()
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
            MOCK_HEADER_HASH.set(None);
        }
    }
}

#[must_use]
pub const fn timestamp_slot(height: u32) -> usize {
    height as usize % Mainnet::PARAMS.window_size
}

#[must_use]
pub const fn timestamp_count(height: u32) -> usize {
    let count = height as usize + 1;
    if count > Mainnet::PARAMS.window_size {
        Mainnet::PARAMS.window_size
    } else {
        count
    }
}

#[must_use]
pub const fn has_ordered_timestamp_window(ordered_timestamp_run: u64) -> bool {
    ordered_timestamp_run >= Mainnet::PARAMS.window_max_index() as u64
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
pub fn ordered_timestamp_run(timestamps: &[BlockTimestamp; Mainnet::PARAMS.window_size], height: u32) -> u64 {
    let count = timestamp_count(height);
    let mut run = 0;
    let mut newer = timestamps[timestamp_slot(height)];
    for offset in 1..count {
        let older = timestamps[timestamp_slot(height + (Mainnet::PARAMS.window_size - offset) as u32)];
        if older > newer {
            break;
        }
        run += 1;
        newer = older;
    }
    run
}

#[must_use]
pub const fn ordered_median_timestamp_slot(height: u32) -> usize {
    timestamp_slot(height + Mainnet::PARAMS.window_large_half_size() as u32)
}

#[must_use]
pub const fn requires_median_hint(height: u64, previous_ordered_timestamp_run: u64) -> bool {
    height >= Mainnet::PARAMS.window_size as u64 && !has_ordered_timestamp_window(previous_ordered_timestamp_run)
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
pub fn calculate_next_target_required<C: Consensus>(
    old_target: Target,
    epoch_start_timestamp: BlockTimestamp,
    previous_timestamp: BlockTimestamp,
) -> (CompactTarget, Target, BlockWork) {
    // Normalize the timespan to ensure it's within the min/max bounds allowed.
    let actual_timespan = i64::from(*previous_timestamp) - i64::from(*epoch_start_timestamp);
    let clamped_timespan = actual_timespan.clamp(C::PARAMS.min_epoch_timespan(), C::PARAMS.max_epoch_timespan()) as u32;

    let mut calculated_target = old_target.into_inner().into_inner();
    calculated_target *= clamped_timespan;

    let expected = C::PARAMS.expected_epoch_time() as u128;
    let mut remainder = 0u128;
    for limb in calculated_target.as_limbs_mut().iter_mut().rev() {
        let value = (remainder << 64) | (*limb as u128);
        *limb = (value / expected) as u64;
        remainder = value % expected;
    }

    // Normalize the new target to ensure an upper-bound (a minimum difficulty) and to handle the implicit
    // truncation that occurs when round-tripping through the nbits format.
    let calculated_target =
        Target::new(NonMaxU256::new(calculated_target).expect("a scaled Bitcoin target cannot be u256::MAX"));
    let clamped_target: Target = calculated_target.min(C::PARAMS.genesis_target);
    let compact_target = target_to_bits(clamped_target);
    let normalized_target = target_from_bits(compact_target);

    let block_work = block_work_from_target(normalized_target);

    // The target is capped above and truncated below; return it and the compact form we calculated along the way.
    (compact_target, normalized_target, block_work)
}
