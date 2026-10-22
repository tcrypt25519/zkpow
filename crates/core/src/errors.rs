use crate::{BlockHash, BlockTimestamp, CompactTarget, ErrorCode, ReturnCode};
use bytemuck::PodCastError;

/// An error that can end the guest program. Transforms typed errors into [`ErrorCode`]s.
pub trait ReportableError {
    /// The code committed for this error, or `None` when the error has no code of its own.
    fn error_code(&self) -> Option<ErrorCode>;

    /// The return code the guest commits for this error. An error with no code of its own still
    /// has to exit non-zero, so it reports [`ErrorCode::UNKNOWN_ERROR`].
    fn return_code(&self) -> ReturnCode {
        ReturnCode::from(self.error_code().unwrap_or(ErrorCode::UNKNOWN_ERROR))
    }
}

/// Reading a fixed-width wire encoding out of a byte slice.
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
    #[error("error")]
    PodCastError(bytemuck::PodCastError),
}

impl From<::bytemuck::PodCastError> for ParseError {
    fn from(value: PodCastError) -> Self {
        ParseError::PodCastError(value)
    }
}

/// Applying headers to a [`State`](crate::State).
///
/// Every variant is a consensus rejection of a specific header, so every variant maps to an
/// [`ErrorCode`]. Prover hints are not validated here: a bad hint halts with the invalid-hint exit
/// code rather than returning, so it never becomes an `ApplyError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ApplyError {
    #[error("header {height} timestamp {timestamp} is not greater than the median {median}")]
    TimestampTooOld {
        height: u32,
        timestamp: BlockTimestamp,
        median: BlockTimestamp,
    },
    #[error("header {height} hash {hash} does not meet the target for nbits {:#010x}", nbits.as_inner())]
    PowInsufficient {
        height: u32,
        hash: BlockHash,
        nbits: CompactTarget,
    },
    #[error("a cold start needs at least {required} headers, got {actual}")]
    InsufficientColdHeaders { required: usize, actual: usize },
}

impl From<zkpow_mtp::errors::InsufficientColdHeaders> for ApplyError {
    fn from(error: zkpow_mtp::errors::InsufficientColdHeaders) -> Self {
        Self::InsufficientColdHeaders {
            required: error.required(),
            actual: error.actual(),
        }
    }
}

impl ReportableError for ApplyError {
    fn error_code(&self) -> Option<ErrorCode> {
        Some(match self {
            Self::TimestampTooOld { .. } => ErrorCode::TIMESTAMP_TOO_OLD,
            Self::PowInsufficient { .. } => ErrorCode::POW_INSUFFICIENT,
            Self::InsufficientColdHeaders { .. } => ErrorCode::INSUFFICIENT_COLD_HEADERS,
        })
    }
}
