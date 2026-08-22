use core::num::NonZeroU8;

use derive_size::Size;

use crate::{
    check_exact_len, BlockHash, BlockTimestamp, ChainWork, CompactTarget, InputParseError, ParseError, State,
    WINDOW_SIZE,
};

/// Digest of a program verifier key.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(
    not(target_os = "zkvm"),
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct VerifierKeyDigest([u32; 8]);

impl VerifierKeyDigest {
    // pub const SIZE: usize = 32;

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

    #[must_use]
    pub fn to_bytes(self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, word) in self.0.iter().enumerate() {
            out[i * 4..(i + 1) * 4].copy_from_slice(&word.to_le_bytes());
        }
        out
    }

    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        let mut words = [0u32; 8];
        for (i, word) in words.iter_mut().enumerate() {
            *word = u32::from_le_bytes(bytes[i * 4..(i + 1) * 4].try_into().unwrap());
        }
        Self(words)
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

/// The verifier-visible portion of a validated chain segment.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Size)]
pub struct Claim {
    pub chain_work: ChainWork,
    pub genesis_hash: BlockHash,
    pub tip_hash: BlockHash,
    pub height: u32,
}

impl Claim {
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::SIZE_NO_TAIL_PAD] {
        let mut out = [0u8; Self::SIZE_NO_TAIL_PAD];
        out[0..32].copy_from_slice(self.chain_work.to_le_bytes_slice());
        out[32..64].copy_from_slice(self.genesis_hash.to_le_bytes_slice());
        out[64..96].copy_from_slice(self.tip_hash.to_le_bytes_slice());
        out[96..100].copy_from_slice(&self.height.to_le_bytes());
        out
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        check_exact_len(bytes, Self::SIZE_NO_TAIL_PAD)?;
        let chain_work = ChainWork::from_le_bytes(bytes[0..32].try_into().unwrap());
        let genesis_hash = BlockHash::new(bytes[32..64].try_into().unwrap());
        let tip_hash = BlockHash::new(bytes[64..96].try_into().unwrap());
        let height = u32::from_le_bytes(bytes[96..100].try_into().unwrap());
        Ok(Self {
            chain_work,
            genesis_hash,
            tip_hash,
            height,
        })
    }
}

/// Continuation data carried forward at each proof step.
///
/// Committed only as a digest in the public values; the raw bytes are
/// supplied as a private witness when extending to the next step.
///
/// Serialized without struct padding (52 bytes):
/// ```text
///  0..  4  current_nbits           u32 LE
///  4..  8  epoch_start_timestamp   u32 LE
///  8.. 52  timestamps              [u32; 11] LE
/// ```
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Size)]
pub struct ContinuationData {
    pub current_nbits: CompactTarget,
    pub epoch_start_timestamp: BlockTimestamp,
    pub timestamps: [BlockTimestamp; WINDOW_SIZE],
}

impl ContinuationData {
    #[must_use]
    pub fn from_state(state: &State) -> Self {
        Self {
            current_nbits: state.current_nbits,
            epoch_start_timestamp: state.epoch_start_timestamp,
            timestamps: state.timestamps,
        }
    }

    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut out = [0u8; Self::SIZE];
        out[0..4].copy_from_slice(self.current_nbits.to_le_bytes_slice());
        out[4..8].copy_from_slice(self.epoch_start_timestamp.to_le_bytes_slice());
        for (i, ts) in self.timestamps.iter().enumerate() {
            out[8 + i * 4..8 + (i + 1) * 4].copy_from_slice(ts.to_le_bytes_slice());
        }
        out
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        check_exact_len(bytes, Self::SIZE)?;
        let current_nbits = CompactTarget::new(u32::from_le_bytes(bytes[0..4].try_into().unwrap()));
        let epoch_start_timestamp = BlockTimestamp::from_le_bytes(bytes[4..8].try_into().unwrap());
        let mut timestamps = [BlockTimestamp::default(); WINDOW_SIZE];
        for (i, ts) in timestamps.iter_mut().enumerate() {
            *ts = BlockTimestamp::from_le_bytes(bytes[8 + i * 4..8 + (i + 1) * 4].try_into().unwrap());
        }
        Ok(Self {
            current_nbits,
            epoch_start_timestamp,
            timestamps,
        })
    }
}

/// A non-zero validation error code.
///
/// `0` is reserved for success, so an `ErrorCode` is always non-zero.
/// Converting a raw `u8` into an `ErrorCode` is fallible because `0` is not a
/// valid error code.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorCode(NonZeroU8);

impl ErrorCode {
    pub const HEADER_PAYLOAD_LENGTH_INVALID: Self = unsafe { Self(NonZeroU8::new_unchecked(1)) };
    pub const POW_INSUFFICIENT: Self = unsafe { Self(NonZeroU8::new_unchecked(2)) };
    pub const TIMESTAMP_TOO_OLD: Self = unsafe { Self(NonZeroU8::new_unchecked(3)) };
    pub const GENESIS_HASH_MISMATCH: Self = unsafe { Self(NonZeroU8::new_unchecked(4)) };
    pub const UNKNOWN_ERROR: Self = unsafe { Self(NonZeroU8::new_unchecked(255)) };

    /// Create an `ErrorCode` from a raw byte, returning `None` for `0`.
    pub const fn new(value: u8) -> Option<Self> {
        match NonZeroU8::new(value) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }

    /// Return the raw byte representation.
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

impl TryFrom<u8> for ErrorCode {
    type Error = InputParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(InputParseError::UnknownErrorCode { code: value })
    }
}

impl From<ErrorCode> for u8 {
    fn from(code: ErrorCode) -> u8 {
        code.get()
    }
}

impl core::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match ErrorCodeNames::try_from(*self) {
            Ok(name) => name.fmt(f),
            Err(_) => write!(f, "unknown error {}", self.get()),
        }
    }
}

/// Named error codes recognized by the validator.
///
/// Other non-zero byte values are valid `ErrorCode`s but map to
/// [`ErrorCodeNames::UnknownError`] when converted by name.
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
#[repr(u8)]
pub enum ErrorCodeNames {
    HeaderPayloadLengthInvalid = 1,
    PowInsufficient = 2,
    TimestampTooOld = 3,
    GenesisHashMismatch = 4,
    UnknownError = 255,
}

impl TryFrom<u8> for ErrorCodeNames {
    type Error = InputParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::HeaderPayloadLengthInvalid),
            2 => Ok(Self::PowInsufficient),
            3 => Ok(Self::TimestampTooOld),
            4 => Ok(Self::GenesisHashMismatch),
            255 => Ok(Self::UnknownError),
            n => Err(InputParseError::UnknownErrorCode { code: n }),
        }
    }
}

impl TryFrom<ErrorCode> for ErrorCodeNames {
    type Error = InputParseError;

    fn try_from(code: ErrorCode) -> Result<Self, Self::Error> {
        Self::try_from(code.get())
    }
}

impl From<ErrorCodeNames> for ErrorCode {
    fn from(name: ErrorCodeNames) -> Self {
        unsafe { Self(NonZeroU8::new_unchecked(name as u8)) }
    }
}

impl core::fmt::Display for ErrorCodeNames {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::HeaderPayloadLengthInvalid => "Header payload length invalid",
            Self::PowInsufficient => "PoW insufficient",
            Self::TimestampTooOld => "Timestamp too old",
            Self::GenesisHashMismatch => "Genesis hash mismatch",
            Self::UnknownError => "Unknown error",
        })
    }
}

/// A validation result: `None` means success, `Some(ErrorCode)` means failure.
///
/// Wire representation is a single `u8`: `0` for success, non-zero for failure.
/// Every `u8` is a valid `ReturnCode`, so conversion from `u8` is infallible.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReturnCode(Option<ErrorCode>);

impl ReturnCode {
    /// Successful validation.
    pub const SUCCESS: Self = Self(None);

    /// Known named error codes.
    pub const HEADER_PAYLOAD_LENGTH_INVALID: Self = Self(Some(ErrorCode::HEADER_PAYLOAD_LENGTH_INVALID));
    pub const POW_INSUFFICIENT: Self = Self(Some(ErrorCode::POW_INSUFFICIENT));
    pub const TIMESTAMP_TOO_OLD: Self = Self(Some(ErrorCode::TIMESTAMP_TOO_OLD));
    pub const GENESIS_HASH_MISMATCH: Self = Self(Some(ErrorCode::GENESIS_HASH_MISMATCH));
    pub const UNKNOWN_ERROR: Self = Self(Some(ErrorCode::UNKNOWN_ERROR));

    /// Create a `ReturnCode` from a raw byte. `0` is success, any non-zero
    /// value is treated as a valid error code.
    pub const fn new(value: u8) -> Self {
        Self(ErrorCode::new(value))
    }

    /// Return `true` if this represents success.
    pub const fn is_success(self) -> bool {
        self.0.is_none()
    }

    /// Return `true` if this represents a failure.
    pub const fn is_failure(self) -> bool {
        self.0.is_some()
    }

    /// Return the underlying error code, if any.
    pub const fn error_code(self) -> Option<ErrorCode> {
        self.0
    }

    /// Return the raw byte representation.
    pub const fn into_u8(self) -> u8 {
        match self.0 {
            Some(code) => code.get(),
            None => 0,
        }
    }
}

impl From<u8> for ReturnCode {
    fn from(value: u8) -> Self {
        Self::new(value)
    }
}

impl From<ErrorCode> for ReturnCode {
    fn from(error: ErrorCode) -> Self {
        Self(Some(error))
    }
}

impl From<ReturnCode> for u8 {
    fn from(code: ReturnCode) -> u8 {
        code.into_u8()
    }
}

impl core::fmt::Display for ReturnCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            None => f.write_str("Success"),
            Some(code) => code.fmt(f),
        }
    }
}

/// Failure information returned by [`State::apply_headers`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyFailure {
    pub error_code: ErrorCode,
}

// impl From<ApplyFailure> for u8 {
//     fn from(value: ApplyFailure) -> u8 {
//         value.i
//     }
// }

/// Wire layout of the minimal public values committed by the prover.
///
/// Layout (165 bytes, little-endian):
/// ```text
///  0.. 32  chain_work          [u8; 32]  (four little-endian u64 limbs)
/// 32.. 64  verifier_key        [u8; 32]  ([u32; 8] LE words)
/// 64.. 96  continuation_digest [u8; 32]
/// 96..128  genesis_hash        [u8; 32]
/// 128..160 tip_hash            [u8; 32]
/// 160..164 height              u32 LE
/// 164      error_code          u8        (0 = success, >0 = failure)
/// ```
///
/// Serialized manually to avoid struct padding; use [`PublicValues::to_bytes`]
/// and [`PublicValues::parse`] rather than transmuting.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimalPublicValues {
    pub chain_work: ChainWork,
    pub verifier_key: VerifierKeyDigest,
    pub continuation_digest: [u8; 32],
    pub genesis_hash: BlockHash,
    pub tip_hash: BlockHash,
    pub height: u32,
    pub return_code: ReturnCode,
}

/// Serialize minimal public values from their individual fields.
///
/// Equivalent to constructing a [`MinimalPublicValues`] and calling `to_bytes`,
/// but avoids materializing the struct.
#[must_use]
pub fn serialize_minimal_public_values(
    chain_work: &ChainWork,
    verifier_key: &VerifierKeyDigest,
    continuation_digest: &[u8; 32],
    genesis_hash: &BlockHash,
    tip_hash: &BlockHash,
    height: u32,
    return_code: u8,
) -> [u8; MinimalPublicValues::SIZE] {
    let mut out = [0u8; MinimalPublicValues::SIZE];
    out[0..32].copy_from_slice(chain_work.to_le_bytes_slice());
    out[32..64].copy_from_slice(&verifier_key.to_bytes());
    out[64..96].copy_from_slice(&continuation_digest[..]);
    out[96..128].copy_from_slice(genesis_hash.to_le_bytes_slice());
    out[128..160].copy_from_slice(tip_hash.to_le_bytes_slice());
    out[160..164].copy_from_slice(&height.to_le_bytes());
    out[164] = return_code;
    out
}

impl MinimalPublicValues {
    pub const SIZE: usize = 165;

    /// Serialize to exactly [`MinimalPublicValues::SIZE`] bytes (no padding).
    #[must_use]
    pub fn to_bytes(&self) -> [u8; MinimalPublicValues::SIZE] {
        serialize_minimal_public_values(
            &self.chain_work,
            &self.verifier_key,
            &self.continuation_digest,
            &self.genesis_hash,
            &self.tip_hash,
            self.height,
            self.return_code.into_u8(),
        )
    }

    /// Deserialize from exactly [`MinimalPublicValues::SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, InputParseError> {
        if bytes.len() != Self::SIZE {
            return Err(InputParseError::InvalidLength {
                expected: Self::SIZE,
                actual: bytes.len(),
            });
        }
        let chain_work = ChainWork::new(bytes[0..32].try_into().unwrap());
        let verifier_key = VerifierKeyDigest::from_bytes(bytes[32..64].try_into().unwrap());
        let continuation_digest: [u8; 32] = bytes[64..96].try_into().unwrap();
        let genesis_hash = BlockHash::new(bytes[96..128].try_into().unwrap());
        let tip_hash = BlockHash::new(bytes[128..160].try_into().unwrap());
        let height = u32::from_le_bytes(bytes[160..164].try_into().unwrap());
        let return_code = ReturnCode::from(bytes[164]);
        Ok(Self {
            chain_work,
            verifier_key,
            continuation_digest,
            genesis_hash,
            tip_hash,
            height,
            return_code,
        })
    }

    /// Return the public claim encoded in these public values.
    #[must_use]
    pub fn claim(&self) -> Claim {
        Claim {
            genesis_hash: self.genesis_hash,
            tip_hash: self.tip_hash,
            chain_work: self.chain_work,
            height: self.height,
        }
    }

    /// Return `true` if the public values report success.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.return_code.is_success()
    }

    /// Return `true` if the public values report a validation failure.
    #[must_use]
    pub fn is_failure(&self) -> bool {
        self.return_code.is_failure()
    }

    /// Return the validation error code if this is a failure, or `None` on success.
    pub fn validation_error_code(&self) -> Option<ErrorCode> {
        self.return_code.error_code()
    }
}
