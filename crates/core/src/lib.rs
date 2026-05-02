//! Shared consensus types and pure helper logic for the zkpow prover.

#![no_std]

extern crate alloc;

use core::{
    mem::{align_of, size_of, MaybeUninit},
    ptr, slice,
};

pub mod input;
pub use input::{
    Input, InputError, InputMut, InputRef, MedianTimePastHintError, MedianTimePastHints,
    MedianTimePastHintsRef, NewHeaderHintError, NewHeaderHints, NewHeaderHintsRef, RecursiveProof,
};

#[cfg(not(target_endian = "little"))]
compile_error!("zkpow wire types require a little-endian target");

/// Execute a closure while emitting stable, report-backed cycle-tracker markers in the guest.
#[cfg(all(target_os = "zkvm", feature = "profiling"))]
#[inline(always)]
pub fn cycle_track_report<T, F>(label: &'static str, f: F) -> T
where
    F: FnOnce() -> T,
{
    sp1_zkvm::io::write(
        1,
        alloc::format!("cycle-tracker-report-start: {label}\n").as_bytes(),
    );
    let output = f();
    sp1_zkvm::io::write(
        1,
        alloc::format!("cycle-tracker-report-end: {label}\n").as_bytes(),
    );
    output
}

/// Execute a closure while preserving the call shape when report-backed
/// profiling is not enabled.
#[cfg(not(all(target_os = "zkvm", feature = "profiling")))]
#[inline(always)]
pub fn cycle_track_report<T, F>(_label: &'static str, f: F) -> T
where
    F: FnOnce() -> T,
{
    f()
}

/// Backwards-compatible helper for existing call sites.
#[inline(always)]
pub fn cycle_track<T, F>(label: &'static str, f: F) -> T
where
    F: FnOnce() -> T,
{
    cycle_track_report(label, f)
}

/// Size of the serialized [`State`] in bytes.
pub const STATE_SIZE: usize = size_of::<State>();
/// Size of a serialized [`RecursiveProof`] in bytes.
pub const RECURSIVE_PROOF_SIZE: usize = size_of::<RecursiveProof>();
/// Size of each [`NewHeader`] input from the prover.
pub const NEW_HEADER_SIZE: usize = size_of::<NewHeader>();

pub const PROOF_CARRYING_STATE_SIZE: usize = RECURSIVE_PROOF_SIZE;

/// Size of a serialized Bitcoin block header in bytes.
pub const BLOCK_HEADER_SIZE: usize = size_of::<Header>();

/// Sliding window size used for median-time-past checks.
pub const WINDOW_SIZE: usize = 11;

/// Mainnet PoW limit in compact form.
pub const GENESIS_NBITS: u32 = 0x1d00ffff;

/// Raw little-endian mainnet genesis block hash.
pub const MAINNET_GENESIS_HASH_RAW: [u8; 32] = [
    0x6f, 0xe2, 0x8c, 0x0a, 0xb6, 0xf1, 0xb3, 0x72, 0xc1, 0xa6, 0xa2, 0x46, 0xae, 0x63, 0xf7, 0x4f,
    0x93, 0x1e, 0x83, 0x65, 0xe1, 0x5a, 0x08, 0x9c, 0x68, 0xd6, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Bitcoin block hash stored in Bitcoin's internal little-endian byte order.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BlockHash([u8; 32]);

impl BlockHash {
    /// Construct from raw little-endian bytes.
    #[must_use]
    pub const fn from_raw(raw: [u8; 32]) -> Self {
        Self(raw)
    }

    /// Borrow the raw little-endian bytes.
    #[must_use]
    pub const fn as_raw(&self) -> &[u8; 32] {
        &self.0
    }

    /// Consume into raw little-endian bytes.
    #[must_use]
    pub const fn into_raw(self) -> [u8; 32] {
        self.0
    }
}

impl From<[u8; 32]> for BlockHash {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl From<BlockHash> for [u8; 32] {
    fn from(value: BlockHash) -> Self {
        value.0
    }
}

/// Bitcoin compact difficulty encoding (`nBits`).
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompactTarget(u32);

impl CompactTarget {
    /// Construct directly from the compact representation.
    #[must_use]
    pub const fn from_consensus(bits: u32) -> Self {
        Self(bits)
    }

    /// Return the compact consensus encoding.
    #[must_use]
    pub const fn to_consensus(self) -> u32 {
        self.0
    }
}

impl From<u32> for CompactTarget {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<CompactTarget> for u32 {
    fn from(value: CompactTarget) -> Self {
        value.0
    }
}

/// Expanded 256-bit proof-of-work target in little-endian byte order.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Target([u8; 32]);

impl Target {
    /// Construct from raw little-endian bytes.
    #[must_use]
    pub const fn from_raw(raw: [u8; 32]) -> Self {
        Self(raw)
    }

    /// Borrow the raw little-endian bytes.
    #[must_use]
    pub const fn as_raw(&self) -> &[u8; 32] {
        &self.0
    }

    /// Consume into raw little-endian bytes.
    #[must_use]
    pub const fn into_raw(self) -> [u8; 32] {
        self.0
    }

    /// Return the cumulative-work increment for one block at this target.
    #[must_use]
    pub fn work(self) -> ChainWork {
        work_from_target(self)
    }
}

impl From<[u8; 32]> for Target {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl From<Target> for [u8; 32] {
    fn from(value: Target) -> Self {
        value.0
    }
}

/// Cumulative proof-of-work as a 256-bit little-endian limb array.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChainWork([u64; 4]);

impl ChainWork {
    /// Construct from little-endian limbs.
    #[must_use]
    pub const fn from_limbs(limbs: [u64; 4]) -> Self {
        Self(limbs)
    }

    /// Borrow the little-endian limbs.
    #[must_use]
    pub const fn as_limbs(&self) -> &[u64; 4] {
        &self.0
    }

    /// Consume into little-endian limbs.
    #[must_use]
    pub const fn into_limbs(self) -> [u64; 4] {
        self.0
    }
}

impl From<[u64; 4]> for ChainWork {
    fn from(value: [u64; 4]) -> Self {
        Self(value)
    }
}

impl From<ChainWork> for [u64; 4] {
    fn from(value: ChainWork) -> Self {
        value.0
    }
}

/// Bitcoin block timestamp encoded as Unix seconds in consensus serialization.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockTimestamp(u32);

impl BlockTimestamp {
    /// Construct directly from the consensus timestamp.
    #[must_use]
    pub const fn from_consensus(timestamp: u32) -> Self {
        Self(timestamp)
    }

    /// Return the raw consensus timestamp.
    #[must_use]
    pub const fn to_consensus(self) -> u32 {
        self.0
    }

    /// Wrapping subtraction matching Bitcoin's timestamp arithmetic.
    #[must_use]
    pub const fn wrapping_sub(self, other: Self) -> u32 {
        self.0.wrapping_sub(other.0)
    }
}

impl From<u32> for BlockTimestamp {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<BlockTimestamp> for u32 {
    fn from(value: BlockTimestamp) -> Self {
        value.0
    }
}

/// A Bitcoin block header with typed fields.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Header {
    /// TODO: Make a newtype for Version.
    /// The maximally correct underlying type is a NonZero<i32>
    pub version: u32,
    pub prev_blockhash: BlockHash,
    pub merkle_root: [u8; 32],
    pub timestamp: BlockTimestamp,
    pub nbits: CompactTarget,
    pub nonce: u32,
}

impl Header {
    /// Serialize to exactly [`BLOCK_HEADER_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; BLOCK_HEADER_SIZE] {
        copy_to_bytes(self)
    }

    /// Deserialize from exactly [`BLOCK_HEADER_SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        cycle_track("parse/header", || copy_from_bytes(bytes))
    }
}

/// Digest of a program verifier key.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VerifierKeyDigest([u32; 8]);

impl VerifierKeyDigest {
    #[must_use]
    pub const fn from_raw(raw: [u32; 8]) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn as_raw(&self) -> &[u32; 8] {
        &self.0
    }

    #[must_use]
    pub const fn into_raw(self) -> [u32; 8] {
        self.0
    }
}

impl From<[u32; 8]> for VerifierKeyDigest {
    fn from(value: [u32; 8]) -> Self {
        Self(value)
    }
}

impl From<VerifierKeyDigest> for [u32; 8] {
    fn from(value: VerifierKeyDigest) -> Self {
        value.0
    }
}

/// Digest of committed public values.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PublicValuesDigest([u8; 32]);

impl PublicValuesDigest {
    #[must_use]
    pub const fn from_raw(raw: [u8; 32]) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn as_raw(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub const fn into_raw(self) -> [u8; 32] {
        self.0
    }
}

impl From<[u8; 32]> for PublicValuesDigest {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl From<PublicValuesDigest> for [u8; 32] {
    fn from(value: PublicValuesDigest) -> Self {
        value.0
    }
}

/// Parse errors for fixed-width serialized core types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("invalid length: expected {expected}, got {actual}")]
    InvalidLength { expected: usize, actual: usize },
    #[error("misaligned input requires {required}-byte alignment")]
    Misaligned { required: usize },
    #[error("truncated input at offset {offset}: need {needed} bytes, got {actual}")]
    Truncated {
        offset: usize,
        needed: usize,
        actual: usize,
    },
}

pub(crate) fn check_exact_len(bytes: &[u8], expected: usize) -> Result<(), ParseError> {
    cycle_track("util/check_exact_len", || {
        if bytes.len() != expected {
            return Err(ParseError::InvalidLength {
                expected,
                actual: bytes.len(),
            });
        }
        Ok(())
    })
}

pub(crate) fn check_aligned<T>(bytes: &[u8]) -> Result<(), ParseError> {
    cycle_track("util/check_aligned", || {
        let required = align_of::<T>();
        let address = bytes.as_ptr() as usize;
        if !address.is_multiple_of(required) {
            return Err(ParseError::Misaligned { required });
        }
        Ok(())
    })
}

pub(crate) fn copy_from_bytes<T>(bytes: &[u8]) -> Result<T, ParseError> {
    cycle_track("util/copy_from_bytes", || {
        check_exact_len(bytes, size_of::<T>())?;
        let mut value = MaybeUninit::<T>::uninit();
        unsafe {
            ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                value.as_mut_ptr() as *mut u8,
                size_of::<T>(),
            );
            Ok(value.assume_init())
        }
    })
}

pub(crate) fn copy_to_bytes<const N: usize, T>(value: &T) -> [u8; N] {
    cycle_track("util/copy_to_bytes", || {
        assert_eq!(size_of::<T>(), N);
        let mut bytes = [0u8; N];
        unsafe {
            ptr::copy_nonoverlapping(value as *const T as *const u8, bytes.as_mut_ptr(), N);
        }
        bytes
    })
}

pub(crate) fn ref_from_bytes<T>(bytes: &[u8]) -> Result<&T, ParseError> {
    cycle_track("util/ref_from_bytes", || {
        check_exact_len(bytes, size_of::<T>())?;
        check_aligned::<T>(bytes)?;
        Ok(unsafe { &*(bytes.as_ptr() as *const T) })
    })
}

pub(crate) fn mut_from_bytes<T>(bytes: &mut [u8]) -> Result<&mut T, ParseError> {
    cycle_track("util/mut_from_bytes", || {
        check_exact_len(bytes, size_of::<T>())?;
        check_aligned::<T>(bytes)?;
        Ok(unsafe { &mut *(bytes.as_mut_ptr() as *mut T) })
    })
}

pub(crate) fn slice_from_bytes<T>(bytes: &[u8]) -> Result<&[T], ParseError> {
    cycle_track("util/slice_from_bytes", || {
        if !bytes.len().is_multiple_of(size_of::<T>()) {
            return Err(ParseError::InvalidLength {
                expected: bytes.len().div_ceil(size_of::<T>()) * size_of::<T>(),
                actual: bytes.len(),
            });
        }
        check_aligned::<T>(bytes)?;
        Ok(unsafe {
            slice::from_raw_parts(bytes.as_ptr() as *const T, bytes.len() / size_of::<T>())
        })
    })
}

/// Prover-supplied fields for a new header.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewHeader {
    pub version: u32,
    pub merkle_root: [u8; 32],
    pub timestamp: BlockTimestamp,
    pub nonce: u32,
}

impl NewHeader {
    /// Extract the prover-supplied subset of fields from a full [`Header`].
    #[must_use]
    pub fn from_header(header: &Header) -> Self {
        Self {
            version: header.version,
            merkle_root: header.merkle_root,
            timestamp: header.timestamp,
            nonce: header.nonce,
        }
    }

    /// Parse a [`NewHeader`] from the flat input buffer at `offset`.
    pub fn parse_at(data: &[u8], offset: usize) -> Result<Self, ParseError> {
        let end = offset
            .checked_add(NEW_HEADER_SIZE)
            .ok_or(ParseError::Truncated {
                offset,
                needed: NEW_HEADER_SIZE,
                actual: data.len().saturating_sub(offset),
            })?;
        let bytes = data.get(offset..end).ok_or(ParseError::Truncated {
            offset,
            needed: NEW_HEADER_SIZE,
            actual: data.len().saturating_sub(offset),
        })?;
        Self::parse(bytes)
    }

    /// Parse a [`NewHeader`] from exactly [`NEW_HEADER_SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        cycle_track("parse/new_header", || copy_from_bytes(bytes))
    }

    /// Borrow a slice of [`NewHeader`] records directly from aligned protocol bytes.
    pub fn slice_from_bytes(bytes: &[u8]) -> Result<&[Self], ParseError> {
        cycle_track("parse/new_header_slice", || slice_from_bytes(bytes))
    }

    /// Serialize to exactly [`NEW_HEADER_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; NEW_HEADER_SIZE] {
        copy_to_bytes(self)
    }

    /// Materialize a full [`Header`] using authenticated chain context.
    #[must_use]
    pub fn into_header(self, prev_blockhash: BlockHash, nbits: CompactTarget) -> Header {
        Header {
            version: self.version,
            prev_blockhash,
            merkle_root: self.merkle_root,
            timestamp: self.timestamp,
            nbits,
            nonce: self.nonce,
        }
    }
}

/// Complete authenticated validation state, serialized between recursive iterations.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub header: Header,
    pub block_hash: BlockHash,
    pub genesis_hash: BlockHash,
    pub next_nbits: CompactTarget,
    pub height: u32,
    pub chain_work: ChainWork,
    pub next_work: ChainWork,
    pub epoch_start_timestamp: BlockTimestamp,
    pub timestamps: [BlockTimestamp; WINDOW_SIZE],
}

impl State {
    /// The number of timestamps currently tracked for median-time-past.
    #[must_use]
    pub fn timestamp_count(&self) -> usize {
        (self.height as usize).min(WINDOW_SIZE)
    }

    /// The circular-buffer slot where the next timestamp should be written.
    #[must_use]
    fn next_timestamp_slot(&self) -> usize {
        (self.height as usize) % WINDOW_SIZE
    }

    /// Serialize to exactly [`STATE_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; STATE_SIZE] {
        copy_to_bytes(self)
    }

    /// Deserialize from exactly [`STATE_SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        cycle_track("parse/state", || copy_from_bytes(bytes))
    }

    /// Borrow a [`State`] directly from aligned protocol bytes.
    pub fn ref_from_bytes(bytes: &[u8]) -> Result<&Self, ParseError> {
        cycle_track("parse/state_ref", || ref_from_bytes(bytes))
    }

    /// The expanded proof-of-work target required for the next header.
    #[must_use]
    pub fn next_target(&self) -> Target {
        bits_to_target(self.next_nbits)
    }

    /// TODO: Delete this. We never sort the window.:q
    /// Return the upper median time past for the currently tracked timestamps.
    #[must_use]
    pub fn median_time_past(&self) -> Option<BlockTimestamp> {
        cycle_track("state/median_time_past", || {
            let height = self.height as usize;
            if height == 0 {
                return None;
            }

            let mut sorted = self.timestamps;
            if height >= WINDOW_SIZE {
                cycle_track("state/median_time_past/sort", || {
                    sorted.sort_unstable();
                });
                return Some(sorted[WINDOW_SIZE / 2]);
            }

            cycle_track("state/median_time_past/sort", || {
                sorted[..height].sort_unstable();
            });
            Some(sorted[height / 2])
        })
    }

    #[must_use]
    fn median_hint_is_valid(&self, claimed_median: BlockTimestamp) -> bool {
        cycle_track("state/validate/median_time_past_hint", || {
            let window_len = self.timestamp_count();
            if window_len == 0 {
                return true;
            }

            let median_index = window_len / 2;

            let (less_count, equal_count, greater_count) =
                cycle_track("state/validate/median_time_past_hint/loop", || {
                    let mut less_count = 0usize;
                    let mut equal_count = 0usize;
                    let mut greater_count = 0usize;
                    for timestamp in self.timestamps.iter().take(window_len) {
                        if *timestamp < claimed_median {
                            less_count += 1;
                        } else if *timestamp > claimed_median {
                            greater_count += 1;
                        } else {
                            equal_count += 1;
                        }
                    }
                    (less_count, equal_count, greater_count)
                });

            cycle_track("state/validate/median_time_past_hint/check_counts", || {
                less_count + equal_count + greater_count == window_len
                    && less_count <= median_index
                    && less_count + equal_count > median_index
            })
        })
    }

    /// Build the next authenticated state from the current state, a prover-supplied header,
    /// and a pre-computed block hash.
    fn next_inner(
        &mut self,
        header: Header,
        block_hash: BlockHash,
        median_time_past: Option<BlockTimestamp>,
        update_chain_work: bool,
    ) -> Result<(), ValidationErrorCode> {
        cycle_track("state/next_inner", || {
            let (required_target, required_work, timestamp_slot) =
                cycle_track("state/next_inner/setup", || {
                    (
                        self.next_target(),
                        self.next_work,
                        self.next_timestamp_slot(),
                    )
                });
            // Validate timestamp
            cycle_track("state/next_inner/validate/median_time_past", || {
                if let Some(median_time_past) = median_time_past.or_else(|| self.median_time_past())
                {
                    if header.timestamp <= median_time_past {
                        return Err(ValidationErrorCode::TimestampTooOld);
                    }
                }
                Ok(())
            })?;

            // Validate pow
            cycle_track("state/next_inner/validate/pow", || {
                if !hash_meets_target(block_hash, required_target) {
                    return Err(ValidationErrorCode::PowInsufficient);
                }
                Ok(())
            })?;

            // Now update self
            cycle_track("state/next_inner/update_height", || {
                self.height += 1;
            });
            cycle_track("state/next_inner/timestamp_window", || {
                self.timestamps[timestamp_slot] = header.timestamp;
            });

            if cycle_track("state/next_innernext_inner/check_epoch_timestamp", || {
                self.height.is_multiple_of(2016)
            }) {
                cycle_track("state/next_inner/epoch_timestamp", || {
                    self.epoch_start_timestamp = header.timestamp;
                });
            }

            if cycle_track("state/next_inner/check_retarget", || {
                (self.height + 1).is_multiple_of(2016)
            }) {
                cycle_track("state/next_inner/retarget", || {
                    let actual_timespan = header.timestamp.wrapping_sub(self.epoch_start_timestamp);
                    let expected_timespan: u32 = 2016 * 600;
                    let clamped = actual_timespan
                        .max(expected_timespan / 4)
                        .min(expected_timespan * 4);
                    // TODO: we could save some cycles by directly using the maximum target
                    let pow_limit = bits_to_target(CompactTarget::from_consensus(GENESIS_NBITS));
                    let mut new_target =
                        retarget_target(required_target, clamped, expected_timespan);
                    if target_gt(new_target, pow_limit) {
                        new_target = pow_limit;
                    }
                    self.next_nbits = target_to_bits(new_target);
                    self.next_work = new_target.work();
                });
            }

            if update_chain_work {
                self.chain_work = cycle_track("state/next_inner/chain_work", || {
                    u256_add(self.chain_work, required_work)
                });
            }
            cycle_track("state/next_inner/assign_state", || {
                self.header = header;
                self.block_hash = block_hash;
            });
            Ok(())
        })
    }

    #[cfg(test)]
    pub fn next<F>(
        &mut self,
        new_header: NewHeader,
        hash_header: F,
    ) -> Result<(), ValidationErrorCode>
    where
        F: FnOnce(&Header) -> BlockHash,
    {
        cycle_track("state/next", || {
            let header = new_header.into_header(self.block_hash, self.next_nbits);
            let block_hash = hash_header(&header);
            self.next_inner(header, block_hash, None, true)
        })
    }

    #[allow(clippy::result_large_err)]
    pub fn apply_headers_in_place<F>(
        &mut self,
        headers: &[NewHeader],
        median_hints: &[BlockTimestamp],
        mut hash_header: F,
    ) -> Result<(), ApplyFailure>
    where
        F: FnMut(&Header) -> BlockHash,
    {
        cycle_track("state/apply_headers", || {
            assert_eq!(
                headers.len(),
                median_hints.len(),
                "median hint count must match header count"
            );

            let mut pending_run_work: Option<ChainWork> = None;
            let mut pending_run_count: u32 = 0;

            let flush_pending_chain_work =
                |state: &mut State, run_work: &mut Option<ChainWork>, run_count: &mut u32| {
                    if let (Some(run_work), count) = (run_work.take(), *run_count) {
                        if count > 0 {
                            cycle_track("state/apply_headers/chain_work_flush", || {
                                let accumulated_work = u256_mul_u32(run_work, count);
                                state.chain_work = u256_add(state.chain_work, accumulated_work);
                            });
                        }
                    }
                    *run_count = 0;
                };

            for (header_index, (new_header, claimed_median)) in headers
                .iter()
                .copied()
                .zip(median_hints.iter().copied())
                .enumerate()
            {
                let required_nbits = self.next_nbits;
                let required_work = self.next_work;
                let header = cycle_track("state/apply_headers/build_header", || {
                    new_header.into_header(self.block_hash, required_nbits)
                });
                let block_hash =
                    cycle_track("state/apply_headers/hash_header", || hash_header(&header));
                let median_time_past = if self.timestamp_count() == 0 {
                    None
                } else {
                    // #[cfg(target_os = "zkvm")]
                    // {
                    //     let window_len = self.timestamp_count();
                    //     let tracked_window: alloc::vec::Vec<u32> = self
                    //         .timestamps
                    //         .iter()
                    //         .take(window_len)
                    //         .map(|timestamp| timestamp.to_consensus())
                    //         .collect();
                    //     sp1_zkvm::io::write(
                    //         1,
                    //         alloc::format!(
                    //             "median-hint-debug: header_index={} height={} claimed_median={} computed_median={:?} tracked_window={:?}\n",
                    //             header_index,
                    //             self.height,
                    //             claimed_median.to_consensus(),
                    //             self.median_time_past().map(|timestamp| timestamp.to_consensus()),
                    //             tracked_window,
                    //         )
                    //         .as_bytes(),
                    //     );
                    // }
                    cycle_track("state/apply_headers/median_hint_check", || {
                        assert!(
                            self.median_hint_is_valid(claimed_median),
                            "invalid median time past hint at header index {}",
                            header_index
                        );
                    });
                    Some(claimed_median)
                };
                if pending_run_work != Some(required_work) {
                    flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);
                    pending_run_work = Some(required_work);
                }

                if let Err(error_code) =
                    self.next_inner(header, block_hash, median_time_past, false)
                {
                    flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);
                    return Err(ApplyFailure {
                        last_valid_state: self.clone(),
                        error_code,
                        failure_height: self.height + 1,
                    });
                }

                pending_run_count += 1;
                if self.next_nbits != required_nbits {
                    flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);
                }
            }

            flush_pending_chain_work(self, &mut pending_run_work, &mut pending_run_count);

            Ok(())
        })
    }

    #[allow(clippy::result_large_err)]
    pub fn apply_headers<F>(
        &self,
        headers: &[NewHeader],
        median_hints: &[BlockTimestamp],
        hash_header: F,
    ) -> Result<Self, ApplyFailure>
    where
        F: FnMut(&Header) -> BlockHash,
    {
        cycle_track("state/apply_headers/clone_wrapper", || {
            let mut state = self.clone();
            state.apply_headers_in_place(headers, median_hints, hash_header)?;
            Ok(state)
        })
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            header: Header::default(),
            block_hash: BlockHash::default(),
            genesis_hash: BlockHash::default(),
            next_nbits: CompactTarget::default(),
            height: 0,
            chain_work: ChainWork::default(),
            next_work: ChainWork::default(),
            epoch_start_timestamp: BlockTimestamp::default(),
            timestamps: [BlockTimestamp::default(); WINDOW_SIZE],
        }
    }
}

// ============================================================================
// Public claim and private continuation types (Step 3)
// ============================================================================

/// The verifier-visible portion of a validated chain segment.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PublicChainClaim {
    pub genesis_hash: BlockHash,
    pub tip_hash: BlockHash,
    pub chain_work: ChainWork,
    pub height: u32,
}

/// Size of a serialized [`PublicChainClaim`] in bytes.
pub const PUBLIC_CHAIN_CLAIM_SIZE: usize = size_of::<PublicChainClaim>();

impl PublicChainClaim {
    #[must_use]
    pub fn to_bytes(&self) -> [u8; PUBLIC_CHAIN_CLAIM_SIZE] {
        copy_to_bytes(self)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        copy_from_bytes(bytes)
    }
}

/// Difficulty triple: expanded target, compact bits, and per-block work.
///
/// Constructed via [`DifficultyState::new`] which validates consistency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DifficultyState {
    pub next_target: Target,
    pub next_nbits: CompactTarget,
    pub next_work: ChainWork,
}

/// Error returned when a [`DifficultyState`] triple is inconsistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DifficultyConsistencyError;

impl DifficultyState {
    /// Construct and validate that `next_target == bits_to_target(next_nbits)`
    /// and `next_work == work_from_target(next_target)`.
    pub fn new(
        next_nbits: CompactTarget,
        next_work: ChainWork,
    ) -> Result<Self, DifficultyConsistencyError> {
        let next_target = bits_to_target(next_nbits);
        let expected_work = work_from_target(next_target);
        if next_work != expected_work {
            return Err(DifficultyConsistencyError);
        }
        Ok(Self {
            next_target,
            next_nbits,
            next_work,
        })
    }

    /// Construct without validation (for trusted internal use).
    #[must_use]
    pub fn from_trusted(next_nbits: CompactTarget, next_work: ChainWork) -> Self {
        Self {
            next_target: bits_to_target(next_nbits),
            next_nbits,
            next_work,
        }
    }
}

/// The private continuation state carried between recursive proof iterations.
///
/// This is committed only as a digest in the public values; the raw bytes are
/// supplied as a private witness when extending a proof.
///
/// Serialized without struct padding (84 bytes):
/// ```text
///  0..  4  next_nbits              u32 LE
///  4.. 36  next_work               [u64; 4] LE
/// 36.. 40  epoch_start_timestamp   u32 LE
/// 40.. 84  timestamps              [u32; 11] LE
/// ```
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateContinuationState {
    pub next_nbits: CompactTarget,
    pub next_work: ChainWork,
    pub epoch_start_timestamp: BlockTimestamp,
    pub timestamps: [BlockTimestamp; WINDOW_SIZE],
}

/// Size of the serialized [`PrivateContinuationState`] in bytes (no padding).
pub const PRIVATE_CONTINUATION_STATE_SIZE: usize = 4 + 32 + 4 + 4 * WINDOW_SIZE;

impl PrivateContinuationState {
    #[must_use]
    pub fn to_bytes(&self) -> [u8; PRIVATE_CONTINUATION_STATE_SIZE] {
        let mut out = [0u8; PRIVATE_CONTINUATION_STATE_SIZE];
        out[0..4].copy_from_slice(&self.next_nbits.to_consensus().to_le_bytes());
        for (i, limb) in self.next_work.as_limbs().iter().enumerate() {
            out[4 + i * 8..4 + (i + 1) * 8].copy_from_slice(&limb.to_le_bytes());
        }
        out[36..40].copy_from_slice(&self.epoch_start_timestamp.to_consensus().to_le_bytes());
        for (i, ts) in self.timestamps.iter().enumerate() {
            out[40 + i * 4..40 + (i + 1) * 4].copy_from_slice(&ts.to_consensus().to_le_bytes());
        }
        out
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        check_exact_len(bytes, PRIVATE_CONTINUATION_STATE_SIZE)?;
        let next_nbits =
            CompactTarget::from_consensus(u32::from_le_bytes(bytes[0..4].try_into().unwrap()));
        let mut limbs = [0u64; 4];
        for (i, limb) in limbs.iter_mut().enumerate() {
            *limb = u64::from_le_bytes(bytes[4 + i * 8..4 + (i + 1) * 8].try_into().unwrap());
        }
        let next_work = ChainWork::from_limbs(limbs);
        let epoch_start_timestamp =
            BlockTimestamp::from_consensus(u32::from_le_bytes(bytes[36..40].try_into().unwrap()));
        let mut timestamps = [BlockTimestamp::default(); WINDOW_SIZE];
        for (i, ts) in timestamps.iter_mut().enumerate() {
            *ts = BlockTimestamp::from_consensus(u32::from_le_bytes(
                bytes[40 + i * 4..40 + (i + 1) * 4].try_into().unwrap(),
            ));
        }
        Ok(Self {
            next_nbits,
            next_work,
            epoch_start_timestamp,
            timestamps,
        })
    }
}

/// Combined public + private validation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationState {
    pub public: PublicChainClaim,
    pub private: PrivateContinuationState,
}

impl ValidationState {
    /// Split a [`State`] into its public claim and private continuation.
    #[must_use]
    pub fn from_state(state: &State) -> Self {
        Self {
            public: PublicChainClaim {
                genesis_hash: state.genesis_hash,
                tip_hash: state.block_hash,
                chain_work: state.chain_work,
                height: state.height,
            },
            private: PrivateContinuationState {
                next_nbits: state.next_nbits,
                next_work: state.next_work,
                epoch_start_timestamp: state.epoch_start_timestamp,
                timestamps: state.timestamps,
            },
        }
    }

    /// Reconstruct a [`State`] from the split representation.
    ///
    /// The `header` and `block_hash` fields of [`State`] are set from the
    /// public claim's `tip_hash`; the full raw header is not available here.
    #[must_use]
    pub fn into_state(self) -> State {
        State {
            header: Header::default(),
            block_hash: self.public.tip_hash,
            genesis_hash: self.public.genesis_hash,
            next_nbits: self.private.next_nbits,
            height: self.public.height,
            chain_work: self.public.chain_work,
            next_work: self.private.next_work,
            epoch_start_timestamp: self.private.epoch_start_timestamp,
            timestamps: self.private.timestamps,
        }
    }
}

/// Validation status emitted by the program on failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ValidationErrorCode {
    HeaderPayloadLengthInvalid = 1,
    PowInsufficient = 2,
    TimestampTooOld = 3,
    GenesisHashMismatch = 4,
}

impl ValidationErrorCode {
    /// Return the committed byte representation for the error code.
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self as u8
    }

    /// Return a human-readable description for the code.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::HeaderPayloadLengthInvalid => "Header payload length invalid",
            Self::PowInsufficient => "PoW insufficient",
            Self::TimestampTooOld => "Timestamp too old",
            Self::GenesisHashMismatch => "Genesis hash mismatch",
        }
    }
}

impl core::fmt::Display for ValidationErrorCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.description())
    }
}

impl TryFrom<u8> for ValidationErrorCode {
    type Error = PublicValuesParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::HeaderPayloadLengthInvalid),
            2 => Ok(Self::PowInsufficient),
            3 => Ok(Self::TimestampTooOld),
            4 => Ok(Self::GenesisHashMismatch),
            _ => Err(PublicValuesParseError::UnknownErrorCode { code: value }),
        }
    }
}

/// Failure payload returned by [`State::apply_headers_in_place`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyFailure {
    pub last_valid_state: State,
    pub error_code: ValidationErrorCode,
    /// Absolute chain height of the failed header (last_valid_height + 1).
    pub failure_height: u32,
}

/// Failure payload in committed public values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofFailure {
    pub error_code: ValidationErrorCode,
    /// Absolute chain height of the failed header (last_valid_height + 1).
    pub failure_height: u32,
}

// ============================================================================
// Minimal public values (Step 6)
// ============================================================================

/// Wire layout of the minimal public values committed by the prover.
///
/// Layout (137 bytes, little-endian):
/// ```text
///  0.. 32  genesis_hash        [u8; 32]
/// 32.. 64  tip_hash            [u8; 32]
/// 64.. 96  chain_work          [u8; 32]  (u256 LE)
/// 96..100  height              u32 LE
/// 100      return_code         u8  (0 = success, nonzero = failure)
/// 101..105 failure_height      u32 LE  (0 on success)
/// 105..137 continuation_digest [u8; 32]
/// ```
///
/// Serialized manually to avoid struct padding; use [`MinimalPublicValues::to_bytes`]
/// and [`MinimalPublicValues::parse`] rather than transmuting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimalPublicValues {
    pub genesis_hash: BlockHash,
    pub tip_hash: BlockHash,
    pub chain_work: [u8; 32],
    pub height: u32,
    pub return_code: u8,
    pub failure_height: u32,
    pub continuation_digest: [u8; 32],
}

/// Size of the serialized [`MinimalPublicValues`] in bytes.
pub const MINIMAL_PV_SIZE: usize = 32 + 32 + 32 + 4 + 1 + 4 + 32; // = 137

impl MinimalPublicValues {
    /// Build success public values from a final state and continuation digest.
    #[must_use]
    pub fn success(state: &State, continuation_digest: [u8; 32]) -> Self {
        let mut chain_work = [0u8; 32];
        for (i, limb) in state.chain_work.as_limbs().iter().enumerate() {
            chain_work[i * 8..(i + 1) * 8].copy_from_slice(&limb.to_le_bytes());
        }
        Self {
            genesis_hash: state.genesis_hash,
            tip_hash: state.block_hash,
            chain_work,
            height: state.height,
            return_code: 0,
            failure_height: 0,
            continuation_digest,
        }
    }

    /// Build failure public values from the last valid state, error, and continuation digest.
    #[must_use]
    pub fn failure(
        state: &State,
        error_code: ValidationErrorCode,
        failure_height: u32,
        continuation_digest: [u8; 32],
    ) -> Self {
        let mut chain_work = [0u8; 32];
        for (i, limb) in state.chain_work.as_limbs().iter().enumerate() {
            chain_work[i * 8..(i + 1) * 8].copy_from_slice(&limb.to_le_bytes());
        }
        Self {
            genesis_hash: state.genesis_hash,
            tip_hash: state.block_hash,
            chain_work,
            height: state.height,
            return_code: error_code.as_byte(),
            failure_height,
            continuation_digest,
        }
    }

    /// Serialize to exactly [`MINIMAL_PV_SIZE`] bytes (no padding).
    #[must_use]
    pub fn to_bytes(&self) -> [u8; MINIMAL_PV_SIZE] {
        let mut out = [0u8; MINIMAL_PV_SIZE];
        out[0..32].copy_from_slice(self.genesis_hash.as_raw());
        out[32..64].copy_from_slice(self.tip_hash.as_raw());
        out[64..96].copy_from_slice(&self.chain_work);
        out[96..100].copy_from_slice(&self.height.to_le_bytes());
        out[100] = self.return_code;
        out[101..105].copy_from_slice(&self.failure_height.to_le_bytes());
        out[105..137].copy_from_slice(&self.continuation_digest);
        out
    }

    /// Deserialize from exactly [`MINIMAL_PV_SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, PublicValuesParseError> {
        if bytes.len() != MINIMAL_PV_SIZE {
            return Err(PublicValuesParseError::InvalidLength {
                actual: bytes.len(),
            });
        }
        let genesis_hash = BlockHash::from_raw(bytes[0..32].try_into().unwrap());
        let tip_hash = BlockHash::from_raw(bytes[32..64].try_into().unwrap());
        let chain_work: [u8; 32] = bytes[64..96].try_into().unwrap();
        let height = u32::from_le_bytes(bytes[96..100].try_into().unwrap());
        let return_code = bytes[100];
        let failure_height = u32::from_le_bytes(bytes[101..105].try_into().unwrap());
        let continuation_digest: [u8; 32] = bytes[105..137].try_into().unwrap();
        Ok(Self {
            genesis_hash,
            tip_hash,
            chain_work,
            height,
            return_code,
            failure_height,
            continuation_digest,
        })
    }

    /// Extract the chain work as a [`ChainWork`].
    #[must_use]
    pub fn chain_work(&self) -> ChainWork {
        let mut limbs = [0u64; 4];
        for (i, limb) in limbs.iter_mut().enumerate() {
            *limb = u64::from_le_bytes(self.chain_work[i * 8..(i + 1) * 8].try_into().unwrap());
        }
        ChainWork::from_limbs(limbs)
    }

    /// Return the error code if this is a failure, or `None` on success.
    pub fn error_code(&self) -> Option<Result<ValidationErrorCode, PublicValuesParseError>> {
        if self.return_code == 0 {
            None
        } else {
            Some(ValidationErrorCode::try_from(self.return_code))
        }
    }
}

/// Typed public values committed by the prover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderChainPublicValues {
    Success {
        claim: PublicChainClaim,
        continuation_digest: [u8; 32],
    },
    Failure {
        failure: ProofFailure,
        last_valid_claim: PublicChainClaim,
        continuation_digest: [u8; 32],
    },
}

impl HeaderChainPublicValues {
    /// Parse committed public values from the minimal 137-byte format.
    pub fn parse(bytes: &[u8]) -> Result<Self, PublicValuesParseError> {
        let pv = MinimalPublicValues::parse(bytes)?;
        let claim = PublicChainClaim {
            genesis_hash: pv.genesis_hash,
            tip_hash: pv.tip_hash,
            chain_work: pv.chain_work(),
            height: pv.height,
        };
        if pv.return_code == 0 {
            Ok(Self::Success {
                claim,
                continuation_digest: pv.continuation_digest,
            })
        } else {
            let error_code = ValidationErrorCode::try_from(pv.return_code)?;
            Ok(Self::Failure {
                failure: ProofFailure {
                    error_code,
                    failure_height: pv.failure_height,
                },
                last_valid_claim: claim,
                continuation_digest: pv.continuation_digest,
            })
        }
    }

    /// Borrow the public claim regardless of success or failure.
    #[must_use]
    pub fn claim(&self) -> &PublicChainClaim {
        match self {
            Self::Success { claim, .. } => claim,
            Self::Failure {
                last_valid_claim, ..
            } => last_valid_claim,
        }
    }

    /// Return the continuation digest.
    #[must_use]
    pub fn continuation_digest(&self) -> &[u8; 32] {
        match self {
            Self::Success {
                continuation_digest,
                ..
            } => continuation_digest,
            Self::Failure {
                continuation_digest,
                ..
            } => continuation_digest,
        }
    }
}

/// Parse errors for committed public values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicValuesParseError {
    InvalidLength { actual: usize },
    UnknownErrorCode { code: u8 },
}

impl core::fmt::Display for PublicValuesParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidLength { actual } => {
                write!(
                    f,
                    "invalid public values length: expected {}, got {}",
                    MINIMAL_PV_SIZE, actual
                )
            }
            Self::UnknownErrorCode { code } => {
                write!(f, "unknown validation error code: {}", code)
            }
        }
    }
}

/// Convert compact `bits` encoding into a 256-bit target.
#[must_use]
pub fn bits_to_target(bits: CompactTarget) -> Target {
    cycle_track("difficulty/bits_to_target", || {
        let bits = bits.to_consensus();
        let exponent = bits >> 24;
        let mantissa = bits & 0x00ff_ffff;
        let mut target = [0u8; 32];
        if mantissa == 0 {
            return Target::from_raw(target);
        }
        let byte_offset = (exponent - 3) as usize;
        if byte_offset < 32 {
            target[byte_offset] = (mantissa & 0xff) as u8;
        }
        if byte_offset + 1 < 32 {
            target[byte_offset + 1] = ((mantissa >> 8) & 0xff) as u8;
        }
        if byte_offset + 2 < 32 {
            target[byte_offset + 2] = ((mantissa >> 16) & 0xff) as u8;
        }
        Target::from_raw(target)
    })
}

/// Convert a full 256-bit target into compact `bits` encoding.
#[must_use]
pub fn target_to_bits(target: Target) -> CompactTarget {
    cycle_track("difficulty/target_to_bits", || {
        let target = target.as_raw();
        let mut high_byte = 31usize;
        while high_byte > 0 && target[high_byte] == 0 {
            high_byte -= 1;
        }
        if target[high_byte] == 0 {
            return CompactTarget::from_consensus(0);
        }
        let bit_length = high_byte * 8 + (8 - target[high_byte].leading_zeros() as usize);
        let nbytes = bit_length.div_ceil(8);
        let mantissa = if high_byte >= 2 {
            (target[high_byte] as u32) << 16
                | (target[high_byte - 1] as u32) << 8
                | target[high_byte - 2] as u32
        } else if high_byte == 1 {
            (target[1] as u32) << 8 | target[0] as u32
        } else {
            target[0] as u32
        };
        let (mantissa, nbytes) = if mantissa & 0x800000 != 0 {
            (mantissa >> 8, nbytes + 1)
        } else {
            (mantissa, nbytes)
        };
        CompactTarget::from_consensus(((nbytes as u32) << 24) | (mantissa & 0x00ff_ffff))
    })
}

impl From<CompactTarget> for Target {
    fn from(value: CompactTarget) -> Self {
        bits_to_target(value)
    }
}

impl From<Target> for CompactTarget {
    fn from(value: Target) -> Self {
        target_to_bits(value)
    }
}

/// Compare two 256-bit little-endian byte arrays.
#[must_use]
fn u256_cmp(lhs: &[u8; 32], rhs: &[u8; 32]) -> core::cmp::Ordering {
    cycle_track("difficulty/u256_le", || {
        for i in (0..32).rev() {
            match lhs[i].cmp(&rhs[i]) {
                core::cmp::Ordering::Equal => continue,
                other => return other,
            }
        }
        core::cmp::Ordering::Equal
    })
}

/// Return `true` when `lhs` is strictly greater than `rhs` as unsigned 256-bit integers.
#[must_use]
pub fn target_gt(lhs: Target, rhs: Target) -> bool {
    cycle_track("difficulty/target_gt", || {
        u256_cmp(lhs.as_raw(), rhs.as_raw()) == core::cmp::Ordering::Greater
    })
}

/// Check whether a header hash satisfies a target.
#[must_use]
pub fn hash_meets_target(hash: BlockHash, target: Target) -> bool    {
    cycle_track("pow/hash_meets_target", || {
        u256_cmp(hash.as_raw(), target.as_raw()) != core::cmp::Ordering::Greater
    })
}

/// Add two little-endian `u256` values.
#[must_use]
pub fn u256_add(a: ChainWork, b: ChainWork) -> ChainWork {
    cycle_track("u256/add", || {
        let a = a.as_limbs();
        let b = b.as_limbs();
        let mut result = [0u64; 4];
        let mut carry = 0u128;
        for i in 0..4 {
            let sum = (a[i] as u128) + (b[i] as u128) + carry;
            result[i] = sum as u64;
            carry = sum >> 64;
        }
        ChainWork::from_limbs(result)
    })
}

/// Multiply a little-endian `u256` by a small scalar.
#[must_use]
pub fn u256_mul_u32(value: ChainWork, multiplier: u32) -> ChainWork {
    cycle_track("u256/mul_u32", || {
        let limbs = value.as_limbs();
        let mut result = [0u64; 4];
        let mut carry = 0u128;
        let multiplier = multiplier as u128;

        for i in 0..4 {
            let product = (limbs[i] as u128) * multiplier + carry;
            result[i] = product as u64;
            carry = product >> 64;
        }

        ChainWork::from_limbs(result)
    })
}

/// TODO: Consider, maximum allowed target is < (#2^256)-1.
/// So adding 1 can't carry into a fifth limb.
/// Also, we could simply do a wrapping add 1.
///   Carry if/only if it wraps to 0.
///   First limb to not wrap stops the carry.
fn target_plus_one(target: Target) -> [u64; 5] {
    cycle_track("pow/work/target_plus_one", || {
        let target = target.as_raw();
        let mut out = [0u64; 5];
        let mut carry = 1u128;
        for (i, limb_out) in out.iter_mut().enumerate().take(4) {
            let base = i * 8;
            let limb = u64::from_le_bytes(target[base..base + 8].try_into().unwrap()) as u128;
            let sum = limb + carry;
            *limb_out = sum as u64;
            carry = sum >> 64;
        }
        out[4] = carry as u64;
        out
    })
}

fn u320_gte(lhs: &[u64; 5], rhs: &[u64; 5]) -> bool {
    cycle_track("pow/work/u320_gte", || {
        for i in (0..5).rev() {
            if lhs[i] > rhs[i] {
                return true;
            }
            if lhs[i] < rhs[i] {
                return false;
            }
        }
        true
    })
}

fn u320_sub_assign(lhs: &mut [u64; 5], rhs: &[u64; 5]) {
    cycle_track("pow/work/u320_sub_assign", || {
        let mut borrow = 0u128;
        for i in 0..5 {
            let rhs = rhs[i] as u128 + borrow;
            let lhs_limb = lhs[i] as u128;
            if lhs_limb >= rhs {
                lhs[i] = (lhs_limb - rhs) as u64;
                borrow = 0;
            } else {
                lhs[i] = ((1u128 << 64) + lhs_limb - rhs) as u64;
                borrow = 1;
            }
        }
    })
}

fn u320_shl1(value: &mut [u64; 5]) {
    cycle_track("pow/work/u320_shl1", || {
        let mut carry = 0u64;
        for limb in value.iter_mut() {
            let next_carry = *limb >> 63;
            *limb = (*limb << 1) | carry;
            carry = next_carry;
        }
    })
}

/// Compute one-block cumulative-work units from the expanded target.
#[must_use]
pub fn work_from_target(target: Target) -> ChainWork {
    cycle_track("pow/work_from_target", || {
        let divisor = target_plus_one(target);
        if divisor == [1, 0, 0, 0, 0] {
            return ChainWork::default();
        }

        let mut remainder = [0u64; 5];
        let mut quotient = [0u64; 4];
        for bit in (0..=256).rev() {
            u320_shl1(&mut remainder);
            if bit == 256 {
                remainder[0] |= 1;
            }
            if u320_gte(&remainder, &divisor) {
                u320_sub_assign(&mut remainder, &divisor);
                if bit < 256 {
                    quotient[(bit / 64) as usize] |= 1u64 << (bit % 64);
                }
            }
        }

        ChainWork::from_limbs(quotient)
    })
}

/// Compute a retargeted 256-bit target from the previous target and measured timespan.
#[must_use]
pub fn retarget_target(old_target: Target, actual_timespan: u32, expected_timespan: u32) -> Target {
    cycle_track("pow/retarget_target", || {
        let old_target = old_target.as_raw();
        let mut old_u64 = [0u64; 4];
        for (i, limb) in old_u64.iter_mut().enumerate() {
            let base = i * 8;
            *limb = u64::from_le_bytes(old_target[base..base + 8].try_into().unwrap());
        }

        let mut product = [0u64; 4];
        let mut carry = 0u128;
        for i in 0..4 {
            let prod = (old_u64[i] as u128) * (actual_timespan as u128) + carry;
            product[i] = prod as u64;
            carry = prod >> 64;
        }

        let mut result = [0u64; 4];
        let mut remainder = 0u128;
        for i in (0..4).rev() {
            let val = (remainder << 64) | (product[i] as u128);
            result[i] = (val / (expected_timespan as u128)) as u64;
            remainder = val % (expected_timespan as u128);
        }

        let mut target = [0u8; 32];
        for i in 0..4 {
            target[i * 8..(i + 1) * 8].copy_from_slice(&result[i].to_le_bytes());
        }
        Target::from_raw(target)
    })
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;
    extern crate std;

    use super::*;

    fn ts(seconds: u32) -> BlockTimestamp {
        BlockTimestamp::from_consensus(seconds)
    }

    fn zero_hash() -> BlockHash {
        BlockHash::from_raw([0; 32])
    }

    /// Convert compact `bits` into cumulative-work units.
    #[must_use]
    pub fn work_from_bits(bits: CompactTarget) -> ChainWork {
        cycle_track("pow/work_from_bits", || Target::from(bits).work())
    }

    fn test_state() -> State {
        State {
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            next_work: work_from_bits(CompactTarget::from_consensus(GENESIS_NBITS)),
            ..State::default()
        }
    }

    fn apply_header(state: &mut State, timestamp: u32) {
        state
            .next(
                NewHeader {
                    version: 1,
                    merkle_root: [0x22; 32],
                    timestamp: ts(timestamp),
                    nonce: 7,
                },
                |_| zero_hash(),
            )
            .unwrap();
    }

    fn median_hints_for_headers(
        initial_state: &State,
        headers: &[NewHeader],
    ) -> Vec<BlockTimestamp> {
        let mut state = initial_state.clone();
        let mut medians = Vec::with_capacity(headers.len());
        for header in headers {
            medians.push(state.median_time_past().unwrap_or_default());
            let timestamp_slot = state.next_timestamp_slot();
            state.timestamps[timestamp_slot] = header.timestamp;
            state.height += 1;
        }
        medians
    }

    fn expected_upper_median(height: u32) -> Option<BlockTimestamp> {
        if height == 0 {
            None
        } else if height < WINDOW_SIZE as u32 {
            Some(ts(height / 2 + 1))
        } else {
            Some(ts(height - (WINDOW_SIZE as u32 / 2)))
        }
    }

    #[test]
    fn fixed_width_wire_sizes_match_protocol() {
        assert_eq!(NEW_HEADER_SIZE, 44);
        assert_eq!(RECURSIVE_PROOF_SIZE, 68);
        assert_eq!(STATE_SIZE, 264);
        assert_eq!(MINIMAL_PV_SIZE, 137);
        assert_eq!(core::mem::align_of::<Header>(), 4);
        assert_eq!(core::mem::align_of::<NewHeader>(), 4);
        assert_eq!(core::mem::align_of::<RecursiveProof>(), 4);
        assert_eq!(core::mem::align_of::<State>(), 8);
    }

    #[test]
    fn new_header_from_header_round_trips_with_into_header() {
        let header = Header {
            version: 7,
            prev_blockhash: BlockHash::from_raw([0x11; 32]),
            merkle_root: [0x22; 32],
            timestamp: BlockTimestamp::from_consensus(123_456),
            nbits: CompactTarget::from_consensus(0x1d00ffff),
            nonce: 99,
        };

        let new_header = NewHeader::from_header(&header);
        assert_eq!(new_header.version, header.version);
        assert_eq!(new_header.merkle_root, header.merkle_root);
        assert_eq!(new_header.timestamp, header.timestamp);
        assert_eq!(new_header.nonce, header.nonce);

        let recovered = new_header.into_header(header.prev_blockhash, header.nbits);
        assert_eq!(recovered, header);
    }

    #[test]
    fn minimal_public_values_round_trips() {
        let state = State {
            block_hash: BlockHash::from_raw([0xAB; 32]),
            genesis_hash: BlockHash::from_raw([0xCD; 32]),
            chain_work: ChainWork::from_limbs([1, 2, 3, 4]),
            height: 42,
            ..State::default()
        };
        let digest = [0x11u8; 32];
        let pv = MinimalPublicValues::success(&state, digest);
        let bytes = pv.to_bytes();
        assert_eq!(bytes.len(), MINIMAL_PV_SIZE);
        let parsed = MinimalPublicValues::parse(&bytes).unwrap();
        assert_eq!(parsed.genesis_hash, state.genesis_hash);
        assert_eq!(parsed.tip_hash, state.block_hash);
        assert_eq!(parsed.height, state.height);
        assert_eq!(parsed.return_code, 0);
        assert_eq!(parsed.failure_height, 0);
        assert_eq!(parsed.continuation_digest, digest);
    }

    #[test]
    fn header_and_new_header_round_trip_exact_wire_bytes() {
        let mut header_bytes = [0u8; BLOCK_HEADER_SIZE];
        header_bytes[0..4].copy_from_slice(&0x1122_3344u32.to_le_bytes());
        header_bytes[4..36].copy_from_slice(&[0x55; 32]);
        header_bytes[36..68].copy_from_slice(&[0x66; 32]);
        header_bytes[68..72].copy_from_slice(&0x7788_99aau32.to_le_bytes());
        header_bytes[72..76].copy_from_slice(&0x1d00_ffffu32.to_le_bytes());
        header_bytes[76..80].copy_from_slice(&0xbbcc_ddeeu32.to_le_bytes());

        let header = Header::parse(&header_bytes).unwrap();
        assert_eq!(header.version, 0x1122_3344);
        assert_eq!(header.prev_blockhash, BlockHash::from_raw([0x55; 32]));
        assert_eq!(header.merkle_root, [0x66; 32]);
        assert_eq!(
            header.timestamp,
            BlockTimestamp::from_consensus(0x7788_99aa)
        );
        assert_eq!(header.nbits, CompactTarget::from_consensus(0x1d00_ffff));
        assert_eq!(header.nonce, 0xbbcc_ddee);
        assert_eq!(header.to_bytes(), header_bytes);

        let mut new_header_bytes = [0u8; NEW_HEADER_SIZE];
        new_header_bytes[0..4].copy_from_slice(&0x1122_3344u32.to_le_bytes());
        new_header_bytes[4..36].copy_from_slice(&[0x66; 32]);
        new_header_bytes[36..40].copy_from_slice(&0x7788_99aau32.to_le_bytes());
        new_header_bytes[40..44].copy_from_slice(&0xbbcc_ddeeu32.to_le_bytes());

        let new_header = NewHeader::parse(&new_header_bytes).unwrap();
        assert_eq!(new_header.version, 0x1122_3344);
        assert_eq!(new_header.merkle_root, [0x66; 32]);
        assert_eq!(
            new_header.timestamp,
            BlockTimestamp::from_consensus(0x7788_99aa)
        );
        assert_eq!(new_header.nonce, 0xbbcc_ddee);
        assert_eq!(new_header.to_bytes(), new_header_bytes);
    }

    #[test]
    fn u256_add_handles_carry_propagation() {
        let a = ChainWork::from_limbs([u64::MAX, 0, 0, 0]);
        let b = ChainWork::from_limbs([1, 0, 0, 0]);

        assert_eq!(u256_add(a, b), ChainWork::from_limbs([0, 1, 0, 0]));
    }

    #[test]
    fn u256_add_wraps_at_256_bits() {
        let a = ChainWork::from_limbs([u64::MAX; 4]);
        let b = ChainWork::from_limbs([1, 0, 0, 0]);

        assert_eq!(u256_add(a, b), ChainWork::default());
    }

    #[test]
    fn u256_mul_u32_scales_by_small_count() {
        let value = ChainWork::from_limbs([3, 0, 0, 0]);

        assert_eq!(u256_mul_u32(value, 7), ChainWork::from_limbs([21, 0, 0, 0]));
    }

    #[test]
    fn hash_meets_target_accepts_exact_target_boundary() {
        let bits = CompactTarget::from_consensus(GENESIS_NBITS);
        let target = bits_to_target(bits);
        let hash = BlockHash::from_raw(target.into_raw());

        assert!(hash_meets_target(hash, target));
    }

    #[test]
    fn apply_headers_flushes_deferred_chain_work_on_success() {
        let state = test_state();
        let headers = [
            NewHeader {
                version: 1,
                merkle_root: [0x22; 32],
                timestamp: ts(10),
                nonce: 7,
            },
            NewHeader {
                version: 1,
                merkle_root: [0x33; 32],
                timestamp: ts(20),
                nonce: 8,
            },
        ];

        let hints = median_hints_for_headers(&state, &headers);
        let result = state
            .apply_headers(&headers, &hints, |_| zero_hash())
            .expect("headers should validate");

        let work = work_from_bits(CompactTarget::from_consensus(GENESIS_NBITS));
        let expected = u256_add(u256_mul_u32(work, 2), ChainWork::default());
        assert_eq!(result.chain_work, expected);
    }

    #[test]
    fn failure_height_is_absolute_chain_height() {
        // Start at height 5 and fail on the first new header → failure_height = 6.
        let state = State {
            height: 5,
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            next_work: work_from_bits(CompactTarget::from_consensus(GENESIS_NBITS)),
            ..State::default()
        };
        let headers = [NewHeader {
            version: 1,
            merkle_root: [0x22; 32],
            timestamp: ts(1000),
            nonce: 7,
        }];
        // Use a hash that exceeds the target to trigger PowInsufficient.
        let failure = state
            .apply_headers(&headers, &[ts(0)], |_| BlockHash::from_raw([0xFF; 32]))
            .expect_err("should fail PoW");

        assert_eq!(failure.error_code, ValidationErrorCode::PowInsufficient);
        assert_eq!(failure.failure_height, 6); // last_valid_state.height(5) + 1
        assert_eq!(failure.last_valid_state.height, 5);
    }

    #[test]
    fn apply_headers_flushes_deferred_chain_work_before_failure() {
        let state = test_state();
        let headers = [
            NewHeader {
                version: 1,
                merkle_root: [0x22; 32],
                timestamp: ts(10),
                nonce: 7,
            },
            NewHeader {
                version: 1,
                merkle_root: [0x33; 32],
                timestamp: ts(0),
                nonce: 8,
            },
        ];

        let hints = median_hints_for_headers(&state, &headers);

        let failure = state
            .apply_headers(&headers, &hints, |_| zero_hash())
            .expect_err("second header should fail timestamp validation");

        let work = work_from_bits(CompactTarget::from_consensus(GENESIS_NBITS));
        assert_eq!(
            failure.last_valid_state.chain_work,
            u256_add(ChainWork::default(), work)
        );
        assert_eq!(failure.failure_height, 2);
    }

    #[test]
    fn apply_headers_matches_sequential_next_across_median_window_wrap() {
        let headers: Vec<NewHeader> = (1..=23)
            .map(|timestamp| NewHeader {
                version: 1,
                merkle_root: [timestamp as u8; 32],
                timestamp: ts(timestamp),
                nonce: timestamp,
            })
            .collect();

        let mut sequential = test_state();
        for header in headers.iter().copied() {
            sequential
                .next(header, |_| zero_hash())
                .expect("sequential validation should succeed");
        }

        let hints = median_hints_for_headers(&test_state(), &headers);
        let batched = test_state()
            .apply_headers(&headers, &hints, |_| zero_hash())
            .expect("batched validation should succeed");

        assert_eq!(batched, sequential);
    }

    #[test]
    fn hinted_apply_headers_matches_sorted_apply_headers_across_window_wrap() {
        let headers: Vec<NewHeader> = (1..=23)
            .map(|timestamp| NewHeader {
                version: 1,
                merkle_root: [timestamp as u8; 32],
                timestamp: ts(timestamp),
                nonce: timestamp,
            })
            .collect();
        let state = test_state();
        let hints = median_hints_for_headers(&state, &headers);

        let hinted = state
            .apply_headers(&headers, &hints, |_| zero_hash())
            .expect("hinted validation should succeed");
        assert_eq!(hinted.height, 23);
    }

    #[test]
    fn hinted_median_validation_accepts_duplicate_median_values() {
        let state = State {
            height: WINDOW_SIZE as u32,
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            next_work: work_from_bits(CompactTarget::from_consensus(GENESIS_NBITS)),
            timestamps: [
                ts(1),
                ts(2),
                ts(3),
                ts(4),
                ts(5),
                ts(6),
                ts(6),
                ts(6),
                ts(7),
                ts(8),
                ts(9),
            ],
            ..State::default()
        };
        let headers = [NewHeader {
            version: 1,
            merkle_root: [0x22; 32],
            timestamp: ts(10),
            nonce: 7,
        }];

        state
            .apply_headers(&headers, &[ts(6)], |_| zero_hash())
            .expect("duplicate median values should be accepted");
    }

    #[test]
    fn hinted_median_validation_rejects_wrong_rank_hint() {
        let state = State {
            height: WINDOW_SIZE as u32,
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            next_work: work_from_bits(CompactTarget::from_consensus(GENESIS_NBITS)),
            timestamps: [
                ts(1),
                ts(2),
                ts(3),
                ts(4),
                ts(5),
                ts(6),
                ts(7),
                ts(8),
                ts(9),
                ts(10),
                ts(11),
            ],
            ..State::default()
        };
        let headers = [NewHeader {
            version: 1,
            merkle_root: [0x22; 32],
            timestamp: ts(12),
            nonce: 7,
        }];

        let result = std::panic::catch_unwind(|| {
            state
                .apply_headers(&headers, &[ts(4)], |_| zero_hash())
                .unwrap();
        });

        assert!(result.is_err());
    }

    #[test]
    fn median_time_past_uses_upper_median_for_heights_zero_through_twelve() {
        let mut state = test_state();

        assert_eq!(state.median_time_past(), expected_upper_median(0));

        for height in 1..=12 {
            apply_header(&mut state, height);
            assert_eq!(state.height, height);
            assert_eq!(state.median_time_past(), expected_upper_median(height));
        }
    }

    #[test]
    fn median_time_past_keeps_upper_median_after_two_wraps() {
        let mut state = test_state();

        for height in 1..=23 {
            apply_header(&mut state, height);
            assert_eq!(state.height, height);
            assert_eq!(state.median_time_past(), expected_upper_median(height));
        }
    }

    #[test]
    fn next_overwrites_only_the_next_ring_slot() {
        let original = [
            ts(10),
            ts(20),
            ts(30),
            ts(40),
            ts(50),
            ts(60),
            ts(70),
            ts(80),
            ts(90),
            ts(100),
            ts(110),
        ];
        let mut state = State {
            block_hash: BlockHash::from_raw([0x11; 32]),
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            next_work: work_from_bits(CompactTarget::from_consensus(GENESIS_NBITS)),
            height: WINDOW_SIZE as u32,
            timestamps: original,
            ..State::default()
        };
        state
            .next(
                NewHeader {
                    version: 1,
                    merkle_root: [0x22; 32],
                    timestamp: ts(999),
                    nonce: 7,
                },
                |_| BlockHash::from_raw([0; 32]), // Use zero hash which meets any target
            )
            .unwrap();

        let mut expected = original;
        expected[0] = ts(999);
        assert_eq!(state.timestamps, expected);
    }

    // =========================================================================
    // Step 3: Public claim and continuation type tests
    // =========================================================================

    #[test]
    fn state_round_trips_through_validation_state() {
        let state = State {
            header: Header::default(),
            block_hash: BlockHash::from_raw([0xAB; 32]),
            genesis_hash: BlockHash::from_raw([0xCD; 32]),
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            height: 42,
            chain_work: ChainWork::from_limbs([1, 2, 3, 4]),
            next_work: ChainWork::from_limbs([5, 6, 7, 8]),
            epoch_start_timestamp: BlockTimestamp::from_consensus(1000),
            timestamps: [BlockTimestamp::from_consensus(100); WINDOW_SIZE],
        };

        let vs = ValidationState::from_state(&state);
        assert_eq!(vs.public.genesis_hash, state.genesis_hash);
        assert_eq!(vs.public.tip_hash, state.block_hash);
        assert_eq!(vs.public.chain_work, state.chain_work);
        assert_eq!(vs.public.height, state.height);
        assert_eq!(vs.private.next_nbits, state.next_nbits);
        assert_eq!(vs.private.next_work, state.next_work);
        assert_eq!(
            vs.private.epoch_start_timestamp,
            state.epoch_start_timestamp
        );
        assert_eq!(vs.private.timestamps, state.timestamps);

        let recovered = vs.into_state();
        // header is zeroed in round-trip (not stored in split form)
        assert_eq!(recovered.block_hash, state.block_hash);
        assert_eq!(recovered.genesis_hash, state.genesis_hash);
        assert_eq!(recovered.next_nbits, state.next_nbits);
        assert_eq!(recovered.height, state.height);
        assert_eq!(recovered.chain_work, state.chain_work);
        assert_eq!(recovered.next_work, state.next_work);
        assert_eq!(recovered.epoch_start_timestamp, state.epoch_start_timestamp);
        assert_eq!(recovered.timestamps, state.timestamps);
    }

    #[test]
    fn private_continuation_state_serializes_to_fixed_width() {
        let pcs = PrivateContinuationState {
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            next_work: ChainWork::from_limbs([1, 2, 3, 4]),
            epoch_start_timestamp: BlockTimestamp::from_consensus(500),
            timestamps: [BlockTimestamp::from_consensus(10); WINDOW_SIZE],
        };
        let bytes = pcs.to_bytes();
        assert_eq!(bytes.len(), PRIVATE_CONTINUATION_STATE_SIZE);
        let parsed = PrivateContinuationState::parse(&bytes).unwrap();
        assert_eq!(parsed, pcs);
    }

    #[test]
    fn public_chain_claim_serializes_to_fixed_width() {
        let claim = PublicChainClaim {
            genesis_hash: BlockHash::from_raw([1; 32]),
            tip_hash: BlockHash::from_raw([2; 32]),
            chain_work: ChainWork::from_limbs([3, 4, 5, 6]),
            height: 99,
        };
        let bytes = claim.to_bytes();
        assert_eq!(bytes.len(), PUBLIC_CHAIN_CLAIM_SIZE);
        let parsed = PublicChainClaim::parse(&bytes).unwrap();
        assert_eq!(parsed, claim);
    }

    #[test]
    fn difficulty_state_rejects_inconsistent_work() {
        let nbits = CompactTarget::from_consensus(GENESIS_NBITS);
        let correct_work = work_from_bits(nbits);
        let wrong_work = ChainWork::from_limbs([0, 0, 0, 0]);

        assert!(DifficultyState::new(nbits, correct_work).is_ok());
        assert!(DifficultyState::new(nbits, wrong_work).is_err());
    }
}
