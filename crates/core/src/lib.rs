//! Shared consensus types and pure helper logic for the zkpow prover.

#![no_std]

extern crate alloc;

use core::{
    marker::PhantomData,
    mem::{align_of, size_of, MaybeUninit},
    ptr, slice,
};

pub mod brand;
pub mod types;

pub use types::{u256, BlockHash, BlockTimestamp, ChainWork, CompactTarget, Target};

pub mod env;
pub use env::{cycle_track, cycle_track_report, State};

#[cfg(feature = "host")]
type DefaultEnvironment = env::HostEnvironment;
#[cfg(not(feature = "host"))]
type DefaultEnvironment = env::GuestEnvironment;

pub mod input;
pub use input::{
    Input, InputError, InputRef, MedianTimePastHintError, MedianTimePastHints,
    MedianTimePastHintsRef, NewHeaderHintError, NewHeaderHints, NewHeaderHintsRef, RecursiveProof,
};

/// Size of the serialized [`State`] in bytes.
pub const STATE_SIZE: usize = size_of::<State>();

/// Size of a serialized [`RecursiveProof`] in bytes.
pub const RECURSIVE_PROOF_SIZE: usize = size_of::<RecursiveProof>();

/// Size of each [`NewHeader`] input from the prover.
pub const NEW_HEADER_SIZE: usize = size_of::<NewHeader>();

pub const PROOF_CARRYING_STATE_SIZE: usize = PUBLIC_CHAIN_CLAIM_SIZE + RECURSIVE_PROOF_SIZE;

/// Size of a serialized Bitcoin block header in bytes.
pub const BLOCK_HEADER_SIZE: usize = size_of::<Header>();

/// Sliding window size used for median-time-past checks.
pub const WINDOW_SIZE: usize = 11;

/// Mainnet PoW limit in compact form.
pub const GENESIS_NBITS: u32 = 0x1d00ffff;

/// Mainnet PoW limit as an expanded 256-bit target (the full expansion of `0x1d00ffff`).
///
/// Stored as four little-endian u64 limbs. The value is:
///   0x00000000FFFF0000000000000000000000000000000000000000000000000000
/// In LE limbs: limbs[3] = 0xFFFF0000, limbs[0..2] = 0.
pub const GENESIS_TARGET: Target = Target::from_limbs([0, 0, 0, 0x0000_0000_FFFF_0000]);

/// Raw little-endian mainnet genesis block hash.
pub const MAINNET_GENESIS_HASH_RAW: [u8; 32] = [
    0x6f, 0xe2, 0x8c, 0x0a, 0xb6, 0xf1, 0xb3, 0x72, 0xc1, 0xa6, 0xa2, 0x46, 0xae, 0x63, 0xf7, 0x4f,
    0x93, 0x1e, 0x83, 0x65, 0xe1, 0x5a, 0x08, 0x9c, 0x68, 0xd6, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Header {
    /// TODO: Make a newtype for Version.
    /// The maximally correct underlying type is a NonZero<i32>
    pub version: u32,
    pub prev_blockhash: BlockHash,
    pub merkle_root: [u8; 32],
    pub timestamp: BlockTimestamp,
    pub compact_target: CompactTarget,
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
    pub fn into_header(self, prev_blockhash: BlockHash, compact_target: CompactTarget) -> Header {
        Header {
            version: self.version,
            prev_blockhash,
            merkle_root: self.merkle_root,
            timestamp: self.timestamp,
            compact_target,
            nonce: self.nonce,
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
pub const PUBLIC_CHAIN_CLAIM_SIZE: usize = 32 + 32 + 32 + 4;

impl PublicChainClaim {
    #[must_use]
    pub fn to_bytes(&self) -> [u8; PUBLIC_CHAIN_CLAIM_SIZE] {
        let mut out = [0u8; PUBLIC_CHAIN_CLAIM_SIZE];
        out[0..32].copy_from_slice(self.genesis_hash.as_raw());
        out[32..64].copy_from_slice(self.tip_hash.as_raw());
        for (i, limb) in self.chain_work.as_limbs().iter().enumerate() {
            out[64 + i * 8..64 + (i + 1) * 8].copy_from_slice(&limb.to_le_bytes());
        }
        out[96..100].copy_from_slice(&self.height.to_le_bytes());
        out
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        check_exact_len(bytes, PUBLIC_CHAIN_CLAIM_SIZE)?;
        let genesis_hash = BlockHash::from_raw(bytes[0..32].try_into().unwrap());
        let tip_hash = BlockHash::from_raw(bytes[32..64].try_into().unwrap());
        let mut work_limbs = [0u64; 4];
        for (i, limb) in work_limbs.iter_mut().enumerate() {
            *limb = u64::from_le_bytes(bytes[64 + i * 8..64 + (i + 1) * 8].try_into().unwrap());
        }
        let chain_work = ChainWork::from_limbs(work_limbs);
        let height = u32::from_le_bytes(bytes[96..100].try_into().unwrap());
        Ok(Self {
            genesis_hash,
            tip_hash,
            chain_work,
            height,
        })
    }
}

/// The private continuation state carried between recursive proof iterations.
///
/// This is committed only as a digest in the public values; the raw bytes are
/// supplied as a private witness when extending a proof.
///
/// Serialized without struct padding (116 bytes):
/// ```text
///  0..  4  next_nbits              u32 LE
///  4.. 36  next_work               [u64; 4] LE
/// 36.. 68  next_target             [u64; 4] LE
/// 68.. 72  epoch_start_timestamp   u32 LE
/// 72..116  timestamps              [u32; 11] LE
/// ```
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateContinuationState {
    pub next_nbits: CompactTarget,
    pub next_work: ChainWork,
    pub next_target: Target,
    pub epoch_start_timestamp: BlockTimestamp,
    pub timestamps: [BlockTimestamp; WINDOW_SIZE],
}

/// Size of the serialized [`PrivateContinuationState`] in bytes (no padding).
pub const PRIVATE_CONTINUATION_STATE_SIZE: usize = 4 + 32 + 32 + 4 + 4 * WINDOW_SIZE;

impl PrivateContinuationState {
    #[must_use]
    pub fn to_bytes(&self) -> [u8; PRIVATE_CONTINUATION_STATE_SIZE] {
        let mut out = [0u8; PRIVATE_CONTINUATION_STATE_SIZE];
        out[0..4].copy_from_slice(&self.next_nbits.to_consensus().to_le_bytes());
        for (i, limb) in self.next_work.as_limbs().iter().enumerate() {
            out[4 + i * 8..4 + (i + 1) * 8].copy_from_slice(&limb.to_le_bytes());
        }
        for (i, limb) in self.next_target.as_limbs().iter().enumerate() {
            out[36 + i * 8..36 + (i + 1) * 8].copy_from_slice(&limb.to_le_bytes());
        }
        out[68..72].copy_from_slice(&self.epoch_start_timestamp.to_le_bytes());
        for (i, ts) in self.timestamps.iter().enumerate() {
            out[72 + i * 4..72 + (i + 1) * 4].copy_from_slice(&ts.to_le_bytes());
        }
        out
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        check_exact_len(bytes, PRIVATE_CONTINUATION_STATE_SIZE)?;
        let next_nbits =
            CompactTarget::from_consensus(u32::from_le_bytes(bytes[0..4].try_into().unwrap()));
        let mut work_limbs = [0u64; 4];
        for (i, limb) in work_limbs.iter_mut().enumerate() {
            *limb = u64::from_le_bytes(bytes[4 + i * 8..4 + (i + 1) * 8].try_into().unwrap());
        }
        let next_work = ChainWork::from_limbs(work_limbs);
        let mut target_limbs = [0u64; 4];
        for (i, limb) in target_limbs.iter_mut().enumerate() {
            *limb = u64::from_le_bytes(bytes[36 + i * 8..36 + (i + 1) * 8].try_into().unwrap());
        }
        let next_target = Target::from_limbs(target_limbs);
        let epoch_start_timestamp =
            BlockTimestamp::from_le_bytes(bytes[68..72].try_into().unwrap());
        let mut timestamps = [BlockTimestamp::default(); WINDOW_SIZE];
        for (i, ts) in timestamps.iter_mut().enumerate() {
            *ts = BlockTimestamp::from_le_bytes(
                bytes[72 + i * 4..72 + (i + 1) * 4].try_into().unwrap(),
            );
        }
        Ok(Self {
            next_nbits,
            next_work,
            next_target,
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
                next_target: state.next_target,
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
            next_target: self.private.next_target,
            epoch_start_timestamp: self.private.epoch_start_timestamp,
            timestamps: self.private.timestamps,
            _environment: PhantomData,
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

/// Failure payload returned by [`State::apply_headers`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyFailure<E: env::Env = DefaultEnvironment> {
    pub last_valid_state: env::StateInner<E>,
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
    // TODO: Use a ProofFailure instead of its components.
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

/// Convert a full 256-bit target into compact `bits` encoding.
#[must_use]
pub fn target_to_bits(target: Target) -> CompactTarget {
    cycle_track("difficulty/target_to_bits", || {
        let bytes = target.to_le_bytes();
        let mut nbytes = 32usize;
        while nbytes > 0 && bytes[nbytes - 1] == 0 {
            nbytes -= 1;
        }
        if nbytes == 0 {
            return CompactTarget::from_consensus(0);
        }

        let mut compact = if nbytes <= 3 {
            let mut value = 0u32;
            for i in (0..nbytes).rev() {
                value = (value << 8) | bytes[i] as u32;
            }
            value << (8 * (3 - nbytes))
        } else {
            ((bytes[nbytes - 1] as u32) << 16)
                | ((bytes[nbytes - 2] as u32) << 8)
                | (bytes[nbytes - 3] as u32)
        };

        if (compact & 0x0080_0000) != 0 {
            compact >>= 8;
            nbytes += 1;
        }

        CompactTarget::from_consensus(((nbytes as u32) << 24) | (compact & 0x007f_ffff))
    })
}

/// Compare two u256 values. Returns `Greater` if `lhs > rhs`.
#[must_use]
fn u256_cmp(lhs: &u256, rhs: &u256) -> core::cmp::Ordering {
    cycle_track("difficulty/u256_cmp", || {
        let a = lhs.as_limbs();
        let b = rhs.as_limbs();
        for i in (0..4).rev() {
            match a[i].cmp(&b[i]) {
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
        u256_cmp(&lhs, &rhs) == core::cmp::Ordering::Greater
    })
}

/// Check whether a header hash satisfies a target.
#[must_use]
pub fn check_proof_of_work(hash: BlockHash, target: Target) -> bool {
    cycle_track("pow/check_proof_of_work", || {
        let hash_u256 = u256::from_le_bytes(*hash.as_raw());
        u256_cmp(&hash_u256, &target) != core::cmp::Ordering::Greater
    })
}

impl core::ops::Add for ChainWork {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        cycle_track("u256/add", || {
            let a = self.as_limbs();
            let b = rhs.as_limbs();
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
}

impl core::ops::Mul<u32> for ChainWork {
    type Output = Self;

    fn mul(self, rhs: u32) -> Self::Output {
        cycle_track("u256/mul_u32", || {
            let limbs = self.as_limbs();
            let mut result = [0u64; 4];
            let mut carry = 0u128;
            let multiplier = rhs as u128;

            for i in 0..4 {
                let product = (limbs[i] as u128) * multiplier + carry;
                result[i] = product as u64;
                carry = product >> 64;
            }

            ChainWork::from_limbs(result)
        })
    }
}

/// TODO: Consider, maximum allowed target is < (#2^256)-1.
/// So adding 1 can't carry into a fifth limb.
/// Also, we could simply do a wrapping add 1.
///   Carry if/only if it wraps to 0.
///   First limb to not wrap stops the carry.
fn target_plus_one(target: Target) -> [u64; 5] {
    cycle_track("pow/work/target_plus_one", || {
        let limbs = target.as_limbs();
        let mut out = [0u64; 5];
        let mut carry = 1u128;
        for (i, limb_out) in out.iter_mut().enumerate().take(4) {
            let sum = limbs[i] as u128 + carry;
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
pub fn calculate_next_work_required(
    old_target: Target,
    actual_timespan: u32,
    expected_timespan: u32,
) -> Target {
    cycle_track("pow/calculate_next_work_required", || {
        let old_u64 = old_target.as_limbs();

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

        Target::from_limbs(result)
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

    /// Convert compact `bits` into cumulative-work units using the genesis constant.
    #[must_use]
    pub fn genesis_work() -> ChainWork {
        work_from_target(GENESIS_TARGET)
    }

    fn test_state() -> State {
        State {
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            next_work: genesis_work(),
            next_target: GENESIS_TARGET,
            ..State::default()
        }
    }

    fn test_median_time_past(state: &State) -> BlockTimestamp {
        let height = state.height as usize;
        if height == 0 {
            return BlockTimestamp::default();
        }

        let mut sorted = state.timestamps;
        if height >= WINDOW_SIZE {
            sorted.sort_unstable();
            sorted[WINDOW_SIZE / 2]
        } else {
            sorted[..height].sort_unstable();
            sorted[height / 2]
        }
    }

    fn apply_header(state: &mut State, timestamp: u32) {
        let median_hint = test_median_time_past(state);
        state
            .apply_headers(
                &[NewHeader {
                    version: 1,
                    merkle_root: [0x22; 32],
                    timestamp: ts(timestamp),
                    nonce: 7,
                }],
                &[median_hint],
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
            medians.push(test_median_time_past(&state));
            let timestamp_slot = state.next_timestamp_slot();
            state.timestamps[timestamp_slot] = header.timestamp;
            state.height += 1;
        }
        medians
    }

    fn failure_public_value_bytes(failure: &ApplyFailure) -> [u8; MINIMAL_PV_SIZE] {
        MinimalPublicValues::failure(
            &failure.last_valid_state,
            failure.error_code,
            failure.failure_height,
            [0xA5; 32],
        )
        .to_bytes()
    }

    fn expected_upper_median(height: u32) -> BlockTimestamp {
        if height == 0 {
            BlockTimestamp::default()
        } else if height < WINDOW_SIZE as u32 {
            ts(height / 2 + 1)
        } else {
            ts(height - (WINDOW_SIZE as u32 / 2))
        }
    }

    #[test]
    fn fixed_width_wire_sizes_match_protocol() {
        assert_eq!(NEW_HEADER_SIZE, 44);
        assert_eq!(RECURSIVE_PROOF_SIZE, 68);
        assert_eq!(STATE_SIZE, 296);
        assert_eq!(PUBLIC_CHAIN_CLAIM_SIZE, 100);
        assert_eq!(PROOF_CARRYING_STATE_SIZE, 168);
        assert_eq!(PRIVATE_CONTINUATION_STATE_SIZE, 116);
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
            compact_target: CompactTarget::from_consensus(0x1d00ffff),
            nonce: 99,
        };

        let new_header = NewHeader::from_header(&header);
        assert_eq!(new_header.version, header.version);
        assert_eq!(new_header.merkle_root, header.merkle_root);
        assert_eq!(new_header.timestamp, header.timestamp);
        assert_eq!(new_header.nonce, header.nonce);

        let recovered = new_header.into_header(header.prev_blockhash, header.compact_target);
        assert_eq!(recovered, header);
    }

    #[test]
    fn minimal_public_values_round_trips() {
        let state: State = State {
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
        assert_eq!(
            header.compact_target,
            CompactTarget::from_consensus(0x1d00_ffff)
        );
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

        assert_eq!(a + b, ChainWork::from_limbs([0, 1, 0, 0]));
    }

    #[test]
    fn u256_add_wraps_at_256_bits() {
        let a = ChainWork::from_limbs([u64::MAX; 4]);
        let b = ChainWork::from_limbs([1, 0, 0, 0]);

        assert_eq!(a + b, ChainWork::default());
    }

    #[test]
    fn u256_mul_u32_scales_by_small_count() {
        let value = ChainWork::from_limbs([3, 0, 0, 0]);

        assert_eq!(value * 7, ChainWork::from_limbs([21, 0, 0, 0]));
    }

    #[test]
    fn check_proof_of_work_accepts_exact_target_boundary() {
        let hash = BlockHash::from_raw(GENESIS_TARGET.to_le_bytes());
        assert!(check_proof_of_work(hash, GENESIS_TARGET));
    }

    #[test]
    fn target_to_bits_round_trips_genesis_target() {
        let round_trip = target_to_bits(GENESIS_TARGET);
        assert_eq!(round_trip.to_consensus(), GENESIS_NBITS);
    }

    #[test]
    fn apply_headers_flushes_deferred_chain_work_on_success() {
        let mut state = test_state();
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
        state
            .apply_headers(&headers, &hints, |_| zero_hash())
            .expect("headers should validate");

        let work = genesis_work();
        let expected = (work * 2) + ChainWork::default();
        assert_eq!(state.chain_work, expected);
    }

    #[test]
    fn apply_headers_flushes_deferred_chain_work_for_longer_run() {
        let mut state = test_state();
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
            NewHeader {
                version: 1,
                merkle_root: [0x44; 32],
                timestamp: ts(30),
                nonce: 9,
            },
        ];

        let hints = median_hints_for_headers(&state, &headers);
        state
            .apply_headers(&headers, &hints, |_| zero_hash())
            .expect("validation should succeed");

        let work = genesis_work();
        assert_eq!(state.chain_work, work * 3);
    }

    #[test]
    fn failure_height_is_absolute_chain_height() {
        // Start at height 5 and fail on the first new header → failure_height = 6.
        let mut state: State = State {
            height: 5,
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            next_work: genesis_work(),
            next_target: GENESIS_TARGET,
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
        let mut state = test_state();
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

        let work = genesis_work();
        assert_eq!(
            failure.last_valid_state.chain_work,
            ChainWork::default() + work
        );
        assert_eq!(failure.failure_height, 2);
    }

    #[test]
    fn apply_headers_failure_output_includes_flushed_chain_work() {
        let mut state = test_state();
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
            .expect_err("validation should fail on the second header");

        let public_values = failure_public_value_bytes(&failure);
        let parsed = MinimalPublicValues::parse(&public_values).unwrap();
        assert_eq!(
            parsed.return_code,
            ValidationErrorCode::TimestampTooOld.as_byte()
        );
        assert_eq!(parsed.failure_height, 2);
        assert_eq!(parsed.chain_work(), failure.last_valid_state.chain_work);
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
            let median_hint = test_median_time_past(&sequential);
            sequential
                .apply_headers(&[header], &[median_hint], |_| zero_hash())
                .expect("sequential validation should succeed");
        }

        let hints = median_hints_for_headers(&test_state(), &headers);
        let mut batched = test_state();
        batched
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

        let mut hinted = state;
        hinted
            .apply_headers(&headers, &hints, |_| zero_hash())
            .expect("hinted validation should succeed");
        assert_eq!(hinted.height, 23);
    }

    #[test]
    fn hinted_median_validation_accepts_duplicate_median_values() {
        let mut state: State = State {
            height: WINDOW_SIZE as u32,
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            next_work: genesis_work(),
            next_target: GENESIS_TARGET,
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
        let state: State = State {
            height: WINDOW_SIZE as u32,
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            next_work: genesis_work(),
            next_target: GENESIS_TARGET,
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
            let mut state = state;
            state
                .apply_headers(&headers, &[ts(4)], |_| zero_hash())
                .unwrap();
        });

        assert!(result.is_err());
    }

    #[test]
    fn apply_headers_treats_height_zero_state_as_trusted_anchor() {
        let mut state = test_state();
        state.genesis_hash = BlockHash::from_raw([0x11; 32]);
        state.block_hash = BlockHash::from_raw([0x22; 32]);
        let headers = [NewHeader {
            version: 1,
            merkle_root: [0x33; 32],
            timestamp: ts(10),
            nonce: 7,
        }];
        let hints = median_hints_for_headers(&state, &headers);
        state
            .apply_headers(&headers, &hints, |_| zero_hash())
            .expect("height-zero anchor should already be trusted");

        assert_eq!(state.height, 1);
        assert_eq!(state.genesis_hash, BlockHash::from_raw([0x11; 32]));
        assert_eq!(state.header.prev_blockhash, BlockHash::from_raw([0x22; 32]));
        assert_eq!(state.block_hash, zero_hash());
    }

    #[test]
    fn median_time_past_uses_upper_median_for_heights_zero_through_twelve() {
        let mut state = test_state();

        assert_eq!(test_median_time_past(&state), expected_upper_median(0));

        for height in 1..=12 {
            apply_header(&mut state, height);
            assert_eq!(state.height, height);
            assert_eq!(test_median_time_past(&state), expected_upper_median(height));
        }
    }

    #[test]
    fn median_time_past_keeps_upper_median_after_two_wraps() {
        let mut state = test_state();

        for height in 1..=23 {
            apply_header(&mut state, height);
            assert_eq!(state.height, height);
            assert_eq!(test_median_time_past(&state), expected_upper_median(height));
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
        let mut state: State = State {
            block_hash: BlockHash::from_raw([0x11; 32]),
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            next_work: genesis_work(),
            next_target: GENESIS_TARGET,
            height: WINDOW_SIZE as u32,
            timestamps: original,
            ..State::default()
        };
        let median_hint = test_median_time_past(&state);
        state
            .apply_headers(
                &[NewHeader {
                    version: 1,
                    merkle_root: [0x22; 32],
                    timestamp: ts(999),
                    nonce: 7,
                }],
                &[median_hint],
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
        let state: State = State {
            header: Header::default(),
            block_hash: BlockHash::from_raw([0xAB; 32]),
            genesis_hash: BlockHash::from_raw([0xCD; 32]),
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            height: 42,
            chain_work: ChainWork::from_limbs([1, 2, 3, 4]),
            next_work: ChainWork::from_limbs([5, 6, 7, 8]),
            next_target: GENESIS_TARGET,
            epoch_start_timestamp: BlockTimestamp::from_consensus(1000),
            timestamps: [BlockTimestamp::from_consensus(100); WINDOW_SIZE],
            _environment: PhantomData,
        };

        let vs = ValidationState::from_state(&state);
        assert_eq!(vs.public.genesis_hash, state.genesis_hash);
        assert_eq!(vs.public.tip_hash, state.block_hash);
        assert_eq!(vs.public.chain_work, state.chain_work);
        assert_eq!(vs.public.height, state.height);
        assert_eq!(vs.private.next_nbits, state.next_nbits);
        assert_eq!(vs.private.next_work, state.next_work);
        assert_eq!(vs.private.next_target, state.next_target);
        assert_eq!(
            vs.private.epoch_start_timestamp,
            state.epoch_start_timestamp
        );
        assert_eq!(vs.private.timestamps, state.timestamps);

        let recovered: State = vs.into_state();
        // header is zeroed in round-trip (not stored in split form)
        assert_eq!(recovered.block_hash, state.block_hash);
        assert_eq!(recovered.genesis_hash, state.genesis_hash);
        assert_eq!(recovered.next_nbits, state.next_nbits);
        assert_eq!(recovered.height, state.height);
        assert_eq!(recovered.chain_work, state.chain_work);
        assert_eq!(recovered.next_work, state.next_work);
        assert_eq!(recovered.next_target, state.next_target);
        assert_eq!(recovered.epoch_start_timestamp, state.epoch_start_timestamp);
        assert_eq!(recovered.timestamps, state.timestamps);
    }

    #[test]
    fn private_continuation_state_serializes_to_fixed_width() {
        let pcs = PrivateContinuationState {
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            next_work: ChainWork::from_limbs([1, 2, 3, 4]),
            next_target: GENESIS_TARGET,
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
        assert_eq!(bytes[64..72], 3u64.to_le_bytes());
        assert_eq!(bytes[72..80], 4u64.to_le_bytes());
        assert_eq!(bytes[80..88], 5u64.to_le_bytes());
        assert_eq!(bytes[88..96], 6u64.to_le_bytes());
        assert_eq!(bytes[96..100], 99u32.to_le_bytes());
        let parsed = PublicChainClaim::parse(&bytes).unwrap();
        assert_eq!(parsed, claim);
    }
}
