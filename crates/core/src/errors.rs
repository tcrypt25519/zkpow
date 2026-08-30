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

/// Validation failure during [`State::apply_headers`] or [`State::apply_headers_from_genesis`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ApplyError {
    #[error("new header timestamp is not greater than the median")]
    TimestampTooOld,
    #[error("new header proof-of-work is insufficient")]
    PowInsufficient,
    #[error("not enough cold headers to fill the median window")]
    InsufficientColdHeaders,
    #[error("median time past hint is invalid")]
    InvalidMedianHint,
}

impl ApplyError {
    #[must_use]
    pub fn error_code(&self) -> crate::ErrorCode {
        match self {
            Self::TimestampTooOld => crate::ErrorCode::TIMESTAMP_TOO_OLD,
            Self::PowInsufficient => crate::ErrorCode::POW_INSUFFICIENT,
            Self::InsufficientColdHeaders => crate::ErrorCode::INSUFFICIENT_COLD_HEADERS,
            Self::InvalidMedianHint => crate::ErrorCode::UNKNOWN_ERROR,
        }
    }
}

/// Parse errors for witness (hints) data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InputParseError {
    #[error("length mismatch: expected {expected} bytes, got {actual}")]
    InvalidLength { expected: usize, actual: usize },
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),
    #[error("unknown error: {code}")]
    UnknownErrorCode { code: u8 },
}
