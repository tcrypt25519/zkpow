use core::num::NonZeroU8;

use crate::{check_exact_len, ParseError, State};

/// Digest of a program verifier key.
#[wiretype::wiretype]
#[repr(C)]
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

/// The non-zero exit status the guest commits when validation fails.
///
/// This is not an error type. It is the wire encoding of a failure, laid out like a Unix exit
/// status: `0` is success, so an `ErrorCode` is always non-zero. It carries a byte and nothing
/// else, which makes it useless for diagnosing a failure and means it belongs only at the point
/// where the guest commits its public values.
///
/// Errors are ordinary enums that carry the context of what went wrong;
/// [`ReportableError`](crate::ReportableError) is how one is reduced to the byte committed here.
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

    /// The name of a code the validator defines, or `None` for any other non-zero byte.
    ///
    /// Codes are read back out of public values produced by other builds of the guest, so a byte
    /// this build does not define is still a valid failure; it just has no name here.
    #[must_use]
    pub const fn name(self) -> Option<&'static str> {
        Some(match self.get() {
            1 => "Header payload length invalid",
            2 => "PoW insufficient",
            3 => "Timestamp too old",
            4 => "Genesis hash mismatch",
            5 => "Insufficient cold headers",
            255 => "Unknown error",
            _ => return None,
        })
    }
}

impl From<ErrorCode> for u8 {
    fn from(code: ErrorCode) -> u8 {
        code.get()
    }
}

impl core::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => write!(f, "Unrecognized error code {}", self.get()),
        }
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

    /// [`PublicValues::to_bytes`] from borrowed parts, without building a `PublicValues`.
    #[must_use]
    pub fn serialize(state: &State, verifier_key: &VerifierKeyDigest, return_code: ReturnCode) -> [u8; Self::SIZE] {
        let mut out = [0u8; Self::SIZE];
        out[0..State::SIZE].copy_from_slice(bytemuck::bytes_of(state));
        out[State::SIZE..State::SIZE + 32].copy_from_slice(verifier_key.to_le_bytes_slice());
        out[Self::SIZE - 1] = return_code.into_u8();
        out
    }

    /// Deserialize from exactly [`PublicValues::SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        check_exact_len(bytes, Self::SIZE)?;
        let state = bytemuck::pod_read_unaligned::<State>(&bytes[0..State::SIZE]);
        let mut verifier_key_bytes = [0; 32];
        verifier_key_bytes.copy_from_slice(&bytes[State::SIZE..State::SIZE + 32]);
        let verifier_key = VerifierKeyDigest::parse(verifier_key_bytes.as_slice());
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
