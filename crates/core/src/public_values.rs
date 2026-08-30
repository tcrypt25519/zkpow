use core::num::NonZeroU8;

use bytemuck::{Pod, Zeroable};

use crate::{check_exact_len, InputParseError, State};

/// Digest of a program verifier key.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Pod, Zeroable)]
#[cfg_attr(
    not(target_os = "zkvm"),
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct VerifierKeyDigest([u32; 8]);

impl VerifierKeyDigest {
    /// The raw wire bytes. Zero-copy; the in-memory layout is already the wire format.
    #[must_use]
    pub fn to_le_bytes_slice(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }

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
        bytemuck::cast::<VerifierKeyDigest, [u8; 32]>(self)
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

/// A non-zero validation error code.
///
/// `0` is reserved for success, so an `ErrorCode` is always non-zero.
/// Converting a raw `u8` into an `ErrorCode` is fallible because `0` is not a
/// valid error code.
///
/// TODO: This type is being used incorrectly.
/// It is not a regular Erorr type, e.g. for a Result<T, E>.
/// It specifically represents the exit status of the guest program when
/// the status is not a successful status.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorCode(NonZeroU8);

impl ErrorCode {
    pub const HEADER_PAYLOAD_LENGTH_INVALID: Self = unsafe { Self(NonZeroU8::new_unchecked(1)) };
    pub const POW_INSUFFICIENT: Self = unsafe { Self(NonZeroU8::new_unchecked(2)) };
    pub const TIMESTAMP_TOO_OLD: Self = unsafe { Self(NonZeroU8::new_unchecked(3)) };
    pub const GENESIS_HASH_MISMATCH: Self = unsafe { Self(NonZeroU8::new_unchecked(4)) };
    pub const INSUFFICIENT_COLD_HEADERS: Self = unsafe { Self(NonZeroU8::new_unchecked(5)) };
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
    PowInsufficient     = 2,
    TimestampTooOld     = 3,
    GenesisHashMismatch = 4,
    UnknownError        = 255,
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
    pub const INSUFFICIENT_COLD_HEADERS: Self = Self(Some(ErrorCode::INSUFFICIENT_COLD_HEADERS));
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

/// Wire layout of the public values committed by the prover.
///
/// Layout (185 bytes, little-endian):
/// ```text
///  0..152  state                 [u8; 152]  complete State bytes
/// 152..184 verifier_key          [u8; 32]   ([u32; 8] LE words)
/// 184      return_code            u8         (0 = success, >0 = failure)
/// ```
///
/// Serialized manually to avoid struct padding; use [`PublicValues::to_bytes`]
/// and [`PublicValues::parse`] rather than transmuting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicValues {
    pub state:        State,
    pub verifier_key: VerifierKeyDigest,
    pub return_code:  ReturnCode,
}

impl PublicValues {
    pub const SIZE: usize = 185;

    /// Create public values from their constituent parts.
    #[must_use]
    pub fn new(state: State, verifier_key: VerifierKeyDigest, return_code: ReturnCode) -> Self {
        Self {
            state,
            verifier_key,
            return_code,
        }
    }

    /// Serialize to exactly [`PublicValues::SIZE`] bytes (no padding).
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        Self::serialize(&self.state, &self.verifier_key, self.return_code)
    }

    #[must_use]
    pub fn serialize(state: &State, verifier_key: &VerifierKeyDigest, return_code: ReturnCode) -> [u8; Self::SIZE] {
        let mut out = [0u8; Self::SIZE];
        out[0..State::SIZE].copy_from_slice(bytemuck::bytes_of(state));
        out[State::SIZE..State::SIZE + 32].copy_from_slice(verifier_key.to_le_bytes_slice());
        out[Self::SIZE - 1] = return_code.into_u8();
        out
    }

    /// Deserialize from exactly [`PublicValues::SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, InputParseError> {
        check_exact_len(bytes, Self::SIZE)?;
        let state = bytemuck::pod_read_unaligned::<State>(&bytes[0..State::SIZE]);
        let mut verifier_key_bytes = [0; 32];
        verifier_key_bytes.copy_from_slice(&bytes[State::SIZE..State::SIZE + 32]);
        let verifier_key = VerifierKeyDigest::from_bytes(verifier_key_bytes);
        let return_code = ReturnCode::from(bytes[Self::SIZE - 1]);
        Ok(Self {
            state,
            verifier_key,
            return_code,
        })
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
