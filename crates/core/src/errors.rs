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

/// Target errors that occur during arithmetic
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TargetError {
    #[error("invalid target: can not be max u256")]
    TargetTooLarge,
}

/// Parse and validation errors for [`ProofCarryingState`](crate::ProofCarryingState).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InputError {
    #[error("input parse error: {0}")]
    Parse(ParseError),
}

impl From<ParseError> for InputError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

/// Parse and validation errors for the new-header private witness batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NewHeaderHintError {
    #[error("new-header hint payload length mismatch: expected {expected} bytes, got {actual}")]
    PayloadLengthInvalid { expected: usize, actual: usize },
    #[error("new-header hint parse error: {0}")]
    Parse(ParseError),
}

/// Parse and validation errors for median-time-past private witness hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MedianTimePastHintError {
    #[error("median hint payload length mismatch: expected {expected} bytes, got {actual}")]
    PayloadLengthInvalid { expected: usize, actual: usize },
    #[error("median hint parse error: {0}")]
    Parse(ParseError),
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
                    crate::MINIMAL_PV_SIZE,
                    actual
                )
            }
            Self::UnknownErrorCode { code } => {
                write!(f, "unknown validation error code: {}", code)
            }
        }
    }
}

impl core::error::Error for PublicValuesParseError {}
