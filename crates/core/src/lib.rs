//! The header chain state transition and the consensus math it runs on.
//!
//! The value types live in `zkpow-types` and the network parameters in `zkpow-params`; this crate
//! is generic over the latter. The types are re-exported here so consumers have a single import
//! surface for a header chain's vocabulary.

#![no_std]
#![cfg_attr(all(test, target_os = "zkvm"), feature(custom_test_frameworks))]
#![cfg_attr(all(test, target_os = "zkvm"), test_runner(zkvm_test::test_runner))]

#[cfg(test)]
mod tests;

extern crate alloc;
extern crate self as zkpow_core;

pub use zkpow_mtp::{
    advance_ordered_timestamp_run, has_ordered_timestamp_window, ordered_median_timestamp_slot, ordered_timestamp_run,
    requires_median_hint, timestamp_count, timestamp_slot, WINDOW_LARGE_HALF_SIZE, WINDOW_MAX_INDEX, WINDOW_SIZE,
    WINDOW_SMALL_HALF_SIZE,
};
use zkpow_params::Consensus;
pub use zkpow_types::{
    u256, BlockHash, BlockTimestamp, BlockWork, ChainWork, CompactTarget, Header, NonMaxU256, Target,
};

mod header;
pub use header::HeaderInput;

mod public_values;
pub use public_values::{ErrorCode, PublicValues, ReturnCode, VerifierKeyDigest};

mod state;
pub use state::State;

mod errors;
pub use errors::{ApplyError, ParseError, ReportableError};

pub mod halt;
mod input;
pub use input::{cast_slice, serialize_inputs};

/// Hashing lives on [`Header`]; this wrapper exists only so tests can substitute a hash without
/// mining one. It must stay in this crate: `cfg(test)` is per-compilation-unit, so a mock in
/// `zkpow-types` would not be compiled when `zkpow-core`'s tests run.
#[cfg(any(test, not(target_os = "zkvm")))]
fn hash_header(header: &Header) -> BlockHash {
    #[cfg(test)]
    if let Some(hash) = test_hash_mock::get() {
        return BlockHash::new(hash);
    }
    BlockHash::from(header)
}

fn hash_header_input(input: &HeaderInput, prev_blockhash_words: &[u64; 8], compact_target: CompactTarget) -> [u64; 8] {
    #[cfg(target_os = "zkvm")]
    {
        #[cfg(test)]
        if let Some(hash) = test_hash_mock::get() {
            return zkpow_sha256::hash_to_state(&hash);
        }
        zkpow_sha256::of_header_words(
            input.version,
            prev_blockhash_words,
            &input.merkle_root,
            *input.timestamp.as_inner(),
            compact_target.into_inner(),
            input.nonce,
        )
    }

    #[cfg(not(target_os = "zkvm"))]
    {
        #[cfg(test)]
        if let Some(hash) = test_hash_mock::get() {
            return zkpow_sha256::hash_to_state(&hash);
        }
        let prev_hash_bytes = zkpow_sha256::state_to_hash(prev_blockhash_words);
        let header = Header {
            version: input.version,
            prev_blockhash: BlockHash::new(prev_hash_bytes),
            merkle_root: input.merkle_root,
            timestamp: input.timestamp,
            compact_target,
            nonce: input.nonce,
        };
        let hash = hash_header(&header);
        zkpow_sha256::hash_to_state(hash.as_inner())
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
pub fn check_proof_of_work(hash: BlockHash, target: &Target) -> bool {
    u256::from_le_bytes(*hash.as_inner()) <= *target.as_inner().as_inner()
}

/// Check whether a header hash in SHA state words satisfies a target.
#[must_use]
fn check_pow_words(words: &[u64; 8], target: &Target) -> bool {
    u256::from_words(words) <= *target.as_inner().as_inner()
}

/// Check whether a header hash in SHA state words satisfies a target.
#[must_use]
pub fn check_proof_of_work_words(words: &[u64; 8], target: &Target) -> bool {
    check_pow_words(words, target)
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

/// Double `value` in place modulo 2^256, returning whether the true product overflowed.
#[must_use]
#[allow(dead_code)]
pub(crate) fn mul2_with_overflow(value: &mut u256) -> bool {
    let limbs = value.as_limbs_mut();
    let overflow = limbs[3] >> 63 != 0;

    #[cfg(target_os = "zkvm")]
    // SAFETY: both pointers are live and 8-byte aligned; the zero modulus means 2^256.
    unsafe {
        static TWO_MOD_2_256: [u64; 8] = [2, 0, 0, 0, 0, 0, 0, 0];
        sp1_lib::syscall_uint256_mulmod(limbs, TWO_MOD_2_256.as_ptr().cast());
    }

    #[cfg(not(target_os = "zkvm"))]
    {
        let mut carry = 0u64;
        for limb in limbs.iter_mut() {
            let next_carry = *limb >> 63;
            *limb = (*limb << 1) | carry;
            carry = next_carry;
        }
    }

    overflow
}

pub(crate) fn uint256_mul_with_carry(a: &[u64; 4], b: &[u64; 4], c: &[u64; 4]) -> ([u64; 4], [u64; 4]) {
    #[cfg(target_os = "zkvm")]
    unsafe {
        let mut lo = [0u64; 4];
        let mut hi = [0u64; 4];
        sp1_lib::syscall_uint256_mul_with_carry(
            a.as_ptr().cast(),
            b.as_ptr().cast(),
            c.as_ptr().cast(),
            lo.as_mut_ptr().cast(),
            hi.as_mut_ptr().cast(),
        );
        (lo, hi)
    }

    #[cfg(not(target_os = "zkvm"))]
    {
        let mut res = [0u128; 8];
        for i in 0..4 {
            res[i] = c[i] as u128;
        }
        for i in 0..4 {
            for j in 0..4 {
                res[i + j] += (a[i] as u128) * (b[j] as u128);
            }
        }
        let mut carry = 0u128;
        let mut out = [0u64; 8];
        for i in 0..8 {
            let total = res[i] + carry;
            out[i] = total as u64;
            carry = total >> 64;
        }
        let mut lo = [0u64; 4];
        let mut hi = [0u64; 4];
        lo.copy_from_slice(&out[0..4]);
        hi.copy_from_slice(&out[4..8]);
        (lo, hi)
    }
}

pub(crate) fn verify_block_work_witness(work: BlockWork, divisor: u256) {
    let w_limbs = work.into_inner().into_limbs();
    let d_limbs = divisor.into_limbs();

    // work = floor(2^256 / divisor) exactly when:
    //   2^256 < (work + 1) * divisor <= 2^256 + divisor
    // Compute work * divisor + divisor in one mul-with-carry syscall.
    let (lo, hi) = uint256_mul_with_carry(&w_limbs, &d_limbs, &d_limbs);
    let upper_hi = hi[1] | hi[2] | hi[3];
    let low_nonzero = lo[0] | lo[1] | lo[2] | lo[3];
    let low = u256::from_limbs(lo);
    if upper_hi != 0 || hi[0] != 1 || low_nonzero == 0 || low > divisor {
        crate::invalid_hint!("block work witness is not the exact quotient");
    }
}

#[must_use]
fn mul2_shift(value: &mut u256) -> bool {
    let limbs = value.as_limbs_mut();
    let mut carry = 0u64;
    for limb in limbs.iter_mut() {
        let next_carry = *limb >> 63;
        *limb = (*limb << 1) | carry;
        carry = next_carry;
    }
    carry > 0
}

/// Compute one-block cumulative-work units from the expanded target using binary long division.
#[must_use]
pub fn compute_block_work_division(target: Target) -> BlockWork {
    let divisor = increment_target(target.into_inner());
    let n = divisor.bits();

    let mut quotient = [0u64; 4];
    let start_bit = 257 - n;

    let mut remainder_limbs = [0u64; 4];
    remainder_limbs[((n - 1) / 64) as usize] = 1u64 << ((n - 1) % 64);
    let mut remainder = u256::from_limbs(remainder_limbs);

    if remainder >= divisor {
        let _ = sub_assign(&mut remainder, divisor);
        if start_bit < 256 {
            quotient[(start_bit / 64) as usize] |= 1u64 << (start_bit % 64);
        }
    }

    for bit in (0..start_bit).rev() {
        let carry = mul2_shift(&mut remainder);
        if carry || remainder >= divisor {
            let _ = sub_assign(&mut remainder, divisor);
            quotient[(bit / 64) as usize] |= 1u64 << (bit % 64);
        }
    }

    BlockWork::from_limbs(quotient)
}

/// Compute one-block cumulative-work units from the expanded target.
///
/// In the zkVM, the quotient `floor(2^256 / (target + 1))` is supplied as a prover hint
/// and verified with one `syscall_uint256_mul_with_carry` call. For divisor `d` and hinted
/// quotient `q`, `q` is exact if and only if `2^256 < (q + 1)d <= 2^256 + d`.
pub fn block_work_from_target(target: Target) -> BlockWork {
    let divisor = increment_target(target.into_inner());

    #[cfg(target_os = "zkvm")]
    {
        sp1_lib::unconstrained!({
            let work = compute_block_work_division(target);
            sp1_lib::io::hint_slice(bytemuck::bytes_of(&work.into_inner().into_limbs()));
        });
        let read_res = unsafe { sp1_lib::read_vec_raw() };
        if read_res.ptr.is_null() || read_res.len != 32 {
            crate::invalid_hint!("failed to read block work hint");
        }
        let limbs: [u64; 4] = unsafe { *(read_res.ptr as *const [u64; 4]) };
        let work = BlockWork::from_limbs(limbs);
        verify_block_work_witness(work, divisor);
        work
    }

    #[cfg(not(target_os = "zkvm"))]
    {
        let work = compute_block_work_division(target);
        verify_block_work_witness(work, divisor);
        work
    }
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
