/// Parse errors for fixed-width serialized core types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("invalid length: expected {expected}, got {actual}")]
    #[from(PodCastError::SizeMismatch)]
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
